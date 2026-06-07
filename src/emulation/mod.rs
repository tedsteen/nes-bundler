use std::{
    fmt::Debug,
    ops::{Deref, DerefMut},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering},
    },
};

use serde::{Deserialize, Serialize};

use thingbuf::{Recycle, ThingBuf};
use winit::event_loop::EventLoopProxy;

use crate::{
    app_event::AppEvent,
    audio::{
        AudioStream,
        pacer::{AudioFlush, AudioProducer},
    },
    emulation::tetanes::TetanesNesState,
    input::JoypadState,
    settings::{MAX_PLAYERS, Settings},
};

pub mod gui;
pub mod tetanes;
pub type LocalNesState = TetanesNesState;
pub fn new_local_nes_state(load_sram: bool) -> LocalNesState {
    // Bound out so the settings write guard is released before `start_rom`, which takes a read
    // guard of its own to restore SRAM.
    let region = Settings::current_mut().nes_region_mut().clone();
    LocalNesState::start_rom(&crate::bundle::Bundle::current().rom, load_sram, &region)
        .expect("Failed to start ROM")
}
pub const NES_WIDTH: u32 = 256;
pub const NES_WIDTH_4_3: u32 = (NES_WIDTH as f32 * (4.0 / 3.0)) as u32;
pub const NES_HEIGHT: u32 = 240;

static NTSC_PAL: &[u8] = include_bytes!("../../config/palette.pal");

pub struct SharedEmulatorState {
    frame: AtomicU32,
}
impl SharedEmulatorState {
    fn new() -> Self {
        Self {
            frame: AtomicU32::new(0),
        }
    }
}

#[derive(Clone)]
pub struct EmulatorControls {
    speed: Arc<AtomicU32>,
    inputs: Arc<[AtomicU8; MAX_PLAYERS]>,
    reset: Arc<AtomicU8>,
    shutdown: Arc<AtomicBool>,
    audio_flush: AudioFlush,
}

impl EmulatorControls {
    fn new(audio_flush: AudioFlush) -> Self {
        Self {
            speed: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            inputs: Arc::new([AtomicU8::new(0), AtomicU8::new(0)]),
            reset: Arc::new(AtomicU8::new(Self::RESET_NONE)),
            shutdown: Arc::new(AtomicBool::new(false)),
            audio_flush,
        }
    }

    const RESET_NONE: u8 = 0;
    const RESET_SOFT: u8 = 1;
    const RESET_HARD: u8 = 2;

    #[cfg_attr(not(feature = "debug"), allow(dead_code))]
    pub fn set_speed(&self, speed: f32) {
        let speed = speed.to_bits();
        if self.speed.swap(speed, Ordering::Relaxed) != speed {
            self.audio_flush.flush();
        }
    }

    pub(crate) fn speed(&self) -> f32 {
        f32::from_bits(self.speed.load(Ordering::Relaxed))
    }

    pub fn write_inputs(&self, joypads: [JoypadState; MAX_PLAYERS]) {
        for (idx, joypad_state) in joypads.into_iter().enumerate() {
            self.inputs[idx].store(*joypad_state, Emulator::INPUT_WRITE_ORDERING);
        }
    }

    fn read_inputs(&self) -> [JoypadState; MAX_PLAYERS] {
        [
            JoypadState(self.inputs[0].load(Emulator::INPUT_READ_ORDERING)),
            JoypadState(self.inputs[1].load(Emulator::INPUT_READ_ORDERING)),
        ]
    }

    pub fn request_reset(&self, hard: bool) {
        let reset = if hard {
            Self::RESET_HARD
        } else {
            Self::RESET_SOFT
        };
        self.reset.fetch_max(reset, Ordering::Release);
        self.audio_flush.flush();
    }

    fn take_reset(&self) -> Option<bool> {
        match self.reset.swap(Self::RESET_NONE, Ordering::AcqRel) {
            Self::RESET_HARD => Some(true),
            Self::RESET_SOFT => Some(false),
            _ => None,
        }
    }

    pub fn request_shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
        self.audio_flush.flush();
    }

    fn shutdown_requested(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }
}

#[derive(Clone)]
pub struct SharedEmulator {
    pub state: Arc<SharedEmulatorState>,
    controls: EmulatorControls,

    pub frame_buffer: VideoBufferPool,
}

#[derive(Clone)]
pub struct SharedState {
    pub emulator: SharedEmulator,
    #[cfg(feature = "netplay")]
    pub netplay: crate::netplay::SharedNetplay,
}

impl SharedEmulator {
    #[cfg_attr(not(feature = "debug"), allow(dead_code))]
    pub fn set_speed(&self, speed: f32) {
        self.controls.set_speed(speed);
    }

    pub fn write_inputs(&self, joypads: [JoypadState; MAX_PLAYERS]) {
        self.controls.write_inputs(joypads);
    }

    pub fn request_reset(&self, hard: bool) {
        self.controls.request_reset(hard);
    }
}

impl SharedState {
    fn new(
        audio_flush: AudioFlush,
        #[cfg(feature = "netplay")] netplay: crate::netplay::SharedNetplay,
    ) -> Self {
        Self {
            emulator: SharedEmulator {
                state: Arc::new(SharedEmulatorState::new()),
                controls: EmulatorControls::new(audio_flush),
                frame_buffer: VideoBufferPool::new(),
            },

            #[cfg(feature = "netplay")]
            netplay,
        }
    }
}

pub struct Emulator {
    pub shared_state: SharedState,
    th: Option<std::thread::JoinHandle<()>>,
}

pub const DEFAULT_SAMPLE_RATE: f32 = 44_100.0;

struct EmulatorRuntime {
    frame_buffer: VideoBufferPool,
    shared_emulator: SharedEmulator,
    event_proxy: EventLoopProxy<AppEvent>,
    audio_producer: AudioProducer,
    #[cfg(feature = "netplay")]
    shared_netplay: crate::netplay::SharedNetplay,
    #[cfg(feature = "netplay")]
    netplay_state_sender: tokio::sync::watch::Sender<crate::netplay::SharedNetplayState>,
    #[cfg(feature = "netplay")]
    netplay_command_rx: tokio::sync::mpsc::Receiver<crate::netplay::NetplayCommand>,
}

impl EmulatorRuntime {
    fn spawn(self, local_nes_state: LocalNesState) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            let EmulatorRuntime {
                frame_buffer,
                shared_emulator,
                event_proxy,
                mut audio_producer,
                #[cfg(feature = "netplay")]
                shared_netplay,
                #[cfg(feature = "netplay")]
                netplay_state_sender,
                #[cfg(feature = "netplay")]
                netplay_command_rx,
            } = self;

            #[allow(unused_mut)]
            let mut nes_state = local_nes_state;
            let controls = shared_emulator.controls.clone();
            #[cfg(feature = "netplay")]
            let mut nes_state = crate::netplay::Netplay::new(
                nes_state,
                shared_netplay,
                netplay_state_sender,
                netplay_command_rx,
            );

            tokio::task::LocalSet::new().block_on(&rt, async move {
                loop {
                    if controls.shutdown_requested() {
                        break;
                    }
                    if let Some(hard) = controls.take_reset() {
                        nes_state.reset(hard);
                    }

                    let mut frame_result = frame_buffer.push_ref();
                    let video_frame_available = frame_result.is_ok();
                    nes_state
                        .advance(
                            controls.read_inputs(),
                            Some(NESBuffers {
                                audio: &mut audio_producer,
                                video: frame_result.as_deref_mut().ok(),
                            }),
                            controls.speed(),
                        )
                        .await;
                    drop(frame_result);

                    if video_frame_available {
                        let _ = event_proxy.send_event(AppEvent::NesFrameReady);
                    }

                    let frame = nes_state.frame();
                    shared_emulator
                        .state
                        .frame
                        .store(frame, Emulator::FRAME_WRITE_ORDERING);

                    //TODO: Figure out why this is needed (probably to avoid busy loop..)
                    tokio::task::yield_now().await;

                    // 2) periodic SRAM snapshot (non-blocking check)
                    if frame.is_multiple_of(Emulator::SRAM_SNAPSHOT_INTERVAL_FRAMES) {
                        use base64::Engine;
                        use base64::engine::general_purpose::STANDARD_NO_PAD as b64;
                        if let Some(sram) = nes_state.save_sram() {
                            let sram = sram.to_vec();
                            // Do this in a blocking task as we want the main loop free from blocking code
                            tokio::task::spawn_blocking(move || {
                                Settings::current_mut().save_state = Some(b64.encode(sram));
                            });
                        }
                    }
                }
            });
        })
    }
}

impl Emulator {
    /// Ordering used when the emulator thread *reads* the inputs written by the main thread.
    /// Paired with `Release` stores in `GameRuntime::write_inputs` to form a proper
    /// acquire-release pair, which is required for correctness on weak memory models (e.g. ARM).
    pub(crate) const INPUT_READ_ORDERING: std::sync::atomic::Ordering =
        std::sync::atomic::Ordering::Acquire;
    const INPUT_WRITE_ORDERING: std::sync::atomic::Ordering = std::sync::atomic::Ordering::Release;
    /// Ordering used when the emulator thread *writes* the frame counter read by other threads.
    const FRAME_WRITE_ORDERING: std::sync::atomic::Ordering = std::sync::atomic::Ordering::Release;
    const SRAM_SNAPSHOT_INTERVAL_FRAMES: u32 = 100;

    pub fn new(audio_stream: &mut AudioStream, event_proxy: EventLoopProxy<AppEvent>) -> Self {
        let nes_state = new_local_nes_state(true);
        let audio_producer = audio_stream.take_producer();
        let audio_flush = audio_producer.flush_handle();

        #[cfg(feature = "netplay")]
        let (shared_netplay, netplay_state_sender, netplay_command_rx) =
            crate::netplay::SharedNetplay::new();

        let shared_state = SharedState::new(
            audio_flush,
            #[cfg(feature = "netplay")]
            shared_netplay,
        );

        let runtime = EmulatorRuntime {
            frame_buffer: shared_state.emulator.frame_buffer.clone(),
            shared_emulator: shared_state.emulator.clone(),
            event_proxy,
            audio_producer,
            #[cfg(feature = "netplay")]
            shared_netplay: shared_state.netplay.clone(),
            #[cfg(feature = "netplay")]
            netplay_state_sender,
            #[cfg(feature = "netplay")]
            netplay_command_rx,
        };
        let thread_handle = runtime.spawn(nes_state);
        Self {
            shared_state,
            th: Some(thread_handle),
        }
    }
}

impl Drop for Emulator {
    fn drop(&mut self) {
        self.shared_state.emulator.controls.request_shutdown();
        if let Some(th) = self.th.take()
            && let Err(e) = th.join()
        {
            log::warn!("Failed to join emulator thread: {e:?}");
        }
    }
}

pub trait NesStateHandler {
    async fn advance(
        &mut self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        buffers: Option<NESBuffers>,
        speed: f32,
    );
    fn reset(&mut self, hard: bool);
    fn save_sram(&mut self) -> Option<&[u8]>;
    fn frame(&self) -> u32;
}

#[derive(Clone, Serialize, Deserialize, Hash, Debug, PartialEq)]
pub enum NesRegion {
    Pal,
    Ntsc,
    Dendy,
}

impl NesRegion {
    pub fn to_fps(&self) -> f32 {
        match self {
            NesRegion::Pal => 50.006_977,
            NesRegion::Ntsc => 60.098_812,
            NesRegion::Dendy => 50.006_977,
        }
    }
}

pub struct NESBuffers<'a> {
    pub audio: &'a mut AudioProducer,
    pub video: Option<&'a mut NESVideoFrame>,
}

pub struct NESVideoFrame(Vec<u8>);

impl NESVideoFrame {
    pub const SIZE: usize = (NES_WIDTH * NES_HEIGHT * 4) as usize;

    /// Allocate a new frame for video output.
    pub fn new() -> Self {
        let mut frame = vec![0; Self::SIZE];
        frame
            .iter_mut()
            .skip(3)
            .step_by(4)
            .for_each(|alpha| *alpha = 255);
        Self(frame)
    }
}

impl Default for NESVideoFrame {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for NESVideoFrame {
    type Target = Vec<u8>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NESVideoFrame {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug)]
pub struct FrameRecycle;

impl Recycle<NESVideoFrame> for FrameRecycle {
    fn new_element(&self) -> NESVideoFrame {
        NESVideoFrame::new()
    }

    fn recycle(&self, _frame: &mut NESVideoFrame) {}
}

#[derive(Debug)]
pub struct VideoBufferPool(Arc<ThingBuf<NESVideoFrame, FrameRecycle>>);

impl VideoBufferPool {
    pub fn new() -> Self {
        Self(Arc::new(ThingBuf::with_recycle(1, FrameRecycle)))
    }
}

impl Deref for VideoBufferPool {
    type Target = Arc<ThingBuf<NESVideoFrame, FrameRecycle>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Clone for VideoBufferPool {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}
