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
        Settings::current_mut().get_nes_region(),
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
    fn new(emulator_command_tx: EmulatorCommandBus) -> Self {
        Self {
            emulator: SharedEmulator {
                command_tx: emulator_command_tx,
                state: Arc::new(SharedEmulatorState::new()),
                inputs: Arc::new([AtomicU8::new(0), AtomicU8::new(0)]),
                frame_buffer: VideoBufferPool::new(),
            },

            #[cfg(feature = "netplay")]
            netplay: crate::netplay::SharedNetplay::new(),
        }
    }
}

pub struct Emulator {
    pub shared_state: SharedState,

    // We need to hold on to this to keep the thread alive
    _th: std::thread::JoinHandle<()>,
}

pub const DEFAULT_SAMPLE_RATE: f32 = 44_100.0;

impl Emulator {
    pub fn new(audio_stream: &mut AudioStream) -> Self {
        #[allow(unused_mut)]
        let mut nes_state = new_local_nes_state(true);

        if let Some(mut audio_producer) = audio_stream.tx.take() {
            let (emulator_tx, emulator_rx) = tokio::sync::mpsc::channel(1);

            let shared_state = SharedState::new(emulator_tx.clone());

            let _th = std::thread::spawn({
                let shared_state = shared_state.clone();

                let mut emulator_rx = emulator_rx;
                move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();
                    let inputs = shared_state.emulator.inputs.clone();
                    let frame_buffer = shared_state.emulator.frame_buffer.clone();
                    let shared_emulator = shared_state.emulator.clone();

                    #[cfg(feature = "netplay")]
                    let mut nes_state =
                        crate::netplay::Netplay::new(nes_state, shared_state.netplay.clone());

                    tokio::task::LocalSet::new().block_on(&rt, async move {
                        loop {
                            // drain pending emulator commands
                            while let Ok(cmd) = emulator_rx.try_recv() {
                                match cmd {
                                    EmulatorCommand::Reset(hard) => nes_state.reset(hard),
                                    EmulatorCommand::SetSpeed(speed) => nes_state.set_speed(speed),
                                }
                            }

                            let mut frame_result = frame_buffer.push_ref();
                            //println!("ADVANCE {:?}", std::time::Instant::now());
                            nes_state
                                .advance(
                                    [
                                        JoypadState(
                                            inputs[0].load(std::sync::atomic::Ordering::Relaxed),
                                        ),
                                        JoypadState(
                                            inputs[1].load(std::sync::atomic::Ordering::Relaxed),
                                        ),
                                    ],
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
                                .store(frame, std::sync::atomic::Ordering::Relaxed);

                            //TODO: Figure out why this is needed (probably to avoid busy loop..)
                            tokio::task::yield_now().await;

                            // 2) periodic SRAM snapshot (non-blocking check)
                            if frame % 100 == 0 {
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
                }
            });
            Self { shared_state, _th }
        } else {
            panic!("No audio producer")
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
