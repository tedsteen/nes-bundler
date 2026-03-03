use std::{
    fmt::Debug,
    ops::{Deref, DerefMut},
    sync::{
        Arc,
        atomic::{AtomicU8, AtomicU32},
    },
};

use serde::{Deserialize, Serialize};

use thingbuf::{Recycle, ThingBuf};
use tokio::sync::mpsc::Sender;

use crate::{
    audio::{AudioStream, pacer::AudioProducer},
    emulation::tetanes::TetanesNesState,
    input::JoypadState,
    settings::{MAX_PLAYERS, Settings},
};

pub mod gui;
pub mod tetanes;
pub type LocalNesState = TetanesNesState;
pub fn new_local_nes_state(load_sram: bool) -> LocalNesState {
    LocalNesState::start_rom(
        &crate::bundle::Bundle::current().rom,
        load_sram,
        Settings::current_mut().nes_region_mut(),
    )
    .expect("Failed to start ROM")
}
pub const NES_WIDTH: u32 = 256;
pub const NES_WIDTH_4_3: u32 = (NES_WIDTH as f32 * (4.0 / 3.0)) as u32;
pub const NES_HEIGHT: u32 = 240;

static NTSC_PAL: &[u8] = include_bytes!("../../config/palette.pal");

#[allow(dead_code)] // Some commands are only sent by certain features
#[derive(Debug)]
pub enum EmulatorCommand {
    Reset(bool),
    SetSpeed(f32),
    Shutdown,
}
pub type EmulatorCommandBus = Sender<EmulatorCommand>;

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
pub struct SharedEmulator {
    pub command_tx: EmulatorCommandBus,
    pub state: Arc<SharedEmulatorState>,

    pub frame_buffer: VideoBufferPool,
    pub inputs: Arc<[AtomicU8; 2]>,
}

#[derive(Clone)]
pub struct SharedState {
    pub emulator: SharedEmulator,
    #[cfg(feature = "netplay")]
    pub netplay: crate::netplay::SharedNetplay,
}
impl SharedState {
    fn new(
        emulator_command_tx: EmulatorCommandBus,
        #[cfg(feature = "netplay")] netplay: crate::netplay::SharedNetplay,
    ) -> Self {
        Self {
            emulator: SharedEmulator {
                command_tx: emulator_command_tx,
                state: Arc::new(SharedEmulatorState::new()),
                inputs: Arc::new([AtomicU8::new(0), AtomicU8::new(0)]),
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
    inputs: Arc<[AtomicU8; MAX_PLAYERS]>,
    frame_buffer: VideoBufferPool,
    shared_emulator: SharedEmulator,
    emulator_rx: tokio::sync::mpsc::Receiver<EmulatorCommand>,
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
                inputs,
                frame_buffer,
                shared_emulator,
                mut emulator_rx,
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
            #[cfg(feature = "netplay")]
            let mut nes_state = crate::netplay::Netplay::new(
                nes_state,
                shared_netplay,
                netplay_state_sender,
                netplay_command_rx,
            );

            tokio::task::LocalSet::new().block_on(&rt, async move {
                loop {
                    if Emulator::drain_pending_commands(&mut emulator_rx, &mut nes_state) {
                        break;
                    }

                    let mut frame_result = frame_buffer.push_ref();
                    nes_state
                        .advance(
                            Emulator::read_input_states(&inputs),
                            Some(NESBuffers {
                                audio: &mut audio_producer,
                                video: frame_result.as_deref_mut().ok(),
                            }),
                        )
                        .await;

                    let frame = nes_state.frame();
                    shared_emulator
                        .state
                        .frame
                        .store(frame, Emulator::FRAME_ORDERING);

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
    const COMMAND_CHANNEL_CAPACITY: usize = 1;
    const INPUT_ORDERING: std::sync::atomic::Ordering = std::sync::atomic::Ordering::Relaxed;
    const FRAME_ORDERING: std::sync::atomic::Ordering = std::sync::atomic::Ordering::Relaxed;
    const SRAM_SNAPSHOT_INTERVAL_FRAMES: u32 = 100;

    fn drain_pending_commands<N: NesStateHandler>(
        emulator_rx: &mut tokio::sync::mpsc::Receiver<EmulatorCommand>,
        nes_state: &mut N,
    ) -> bool {
        let mut should_shutdown = false;
        while let Ok(cmd) = emulator_rx.try_recv() {
            match cmd {
                EmulatorCommand::Reset(hard) => nes_state.reset(hard),
                EmulatorCommand::SetSpeed(speed) => nes_state.set_speed(speed),
                EmulatorCommand::Shutdown => {
                    should_shutdown = true;
                }
            }
        }
        should_shutdown
    }

    fn read_input_states(inputs: &[AtomicU8; MAX_PLAYERS]) -> [JoypadState; MAX_PLAYERS] {
        [
            JoypadState(inputs[0].load(Self::INPUT_ORDERING)),
            JoypadState(inputs[1].load(Self::INPUT_ORDERING)),
        ]
    }

    pub fn new(audio_stream: &mut AudioStream) -> Self {
        let nes_state = new_local_nes_state(true);
        let audio_producer = audio_stream.take_producer();
        let (emulator_tx, emulator_rx) = tokio::sync::mpsc::channel(Self::COMMAND_CHANNEL_CAPACITY);

        #[cfg(feature = "netplay")]
        let (shared_netplay, netplay_state_sender, netplay_command_rx) =
            crate::netplay::SharedNetplay::new();

        let shared_state = SharedState::new(
            emulator_tx.clone(),
            #[cfg(feature = "netplay")]
            shared_netplay,
        );

        let runtime = EmulatorRuntime {
            inputs: shared_state.emulator.inputs.clone(),
            frame_buffer: shared_state.emulator.frame_buffer.clone(),
            shared_emulator: shared_state.emulator.clone(),
            emulator_rx,
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
        let _ = self
            .shared_state
            .emulator
            .command_tx
            .blocking_send(EmulatorCommand::Shutdown);
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
    );
    fn reset(&mut self, hard: bool);
    fn set_speed(&mut self, speed: f32);
    fn save_sram(&self) -> Option<&[u8]>;
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
