use std::{
    ops::{Deref, DerefMut},
    sync::{Arc, RwLock, atomic::AtomicU8},
    time::Instant,
};

use anyhow::Result;

use ringbuf::traits::Observer;
use serde::{Deserialize, Serialize};

use thingbuf::{Recycle, ThingBuf};
use tokio::sync::mpsc::Sender;

use crate::{
    audio::{AudioProducer, AudioStream},
    input::JoypadState,
    netplay::connecting_state::{StartMethod, SynchonizingState},
    settings::{MAX_PLAYERS, Settings},
};

pub mod gui;
pub mod tetanes;
use self::tetanes::TetanesNesState;
pub type LocalNesState = TetanesNesState;

pub const NES_WIDTH: u32 = 256;
pub const NES_WIDTH_4_3: u32 = (NES_WIDTH as f32 * (4.0 / 3.0)) as u32;
pub const NES_HEIGHT: u32 = 240;

static NTSC_PAL: &[u8] = include_bytes!("../../config/palette.pal");

#[cfg(feature = "netplay")]
pub type StateHandler = crate::netplay::NetplayStateHandler;
#[cfg(not(feature = "netplay"))]
pub type StateHandler = crate::emulation::LocalNesState;

#[allow(dead_code)] // Some commands are only sent by certain features
#[derive(Debug)]
pub enum EmulatorCommand {
    Reset(bool),
    SetSpeed(f32),
}
pub type EmulatorCommandBus = Sender<EmulatorCommand>;

pub enum SharedNetplayConnectingState {
    Connecting,
    LoadingNetplayServerConfiguration(StartMethod),
    PeeringUp(StartMethod),
    Synchronizing(SynchonizingState),
    Failed(String),
    Retry,
}

pub struct SharedNetplayConnectedState {
    pub start_time: Instant,
}

pub enum SharedNetplayState {
    Disconnected,
    Connecting(SharedNetplayConnectingState),
    Connected(SharedNetplayConnectedState),
    Resuming,
    Failed(String),
}

impl SharedNetplayState {
    fn new() -> Self {
        Self::Disconnected
    }
}
pub struct SharedEmulatorState {
    frame: u32,
}
impl SharedEmulatorState {
    fn new() -> Self {
        Self { frame: 0 }
    }
}

#[derive(Debug)]
pub enum NetplayCommand {
    JoinGame(String),
    FindGame,
    HostGame,

    CancelConnect,
    RetryConnect(StartMethod),

    Resume,
    Disconnect,
}
pub type NetplayCommandBus = Sender<NetplayCommand>;

pub struct SharedState {
    pub emulator_state: Arc<RwLock<SharedEmulatorState>>,
    pub emulator_command_tx: EmulatorCommandBus,

    pub netplay_state: Arc<RwLock<SharedNetplayState>>,
    pub netplay_command_tx: NetplayCommandBus,
}
impl SharedState {
    fn new(emulator_command_tx: EmulatorCommandBus, netplay_command_tx: NetplayCommandBus) -> Self {
        Self {
            emulator_state: Arc::new(RwLock::new(SharedEmulatorState::new())),
            emulator_command_tx: emulator_command_tx,
            netplay_state: Arc::new(RwLock::new(SharedNetplayState::new())),
            netplay_command_tx: netplay_command_tx,
        }
    }
}

pub struct Emulator {
    pub frame_buffer: VideoBufferPool,
    pub shared_inputs: Arc<[AtomicU8; 2]>,

    pub shared_state: SharedState,

    pub audio_stream: AudioStream,
    rt: tokio::runtime::Runtime,
}

pub const SAMPLE_RATE: f32 = 44_100.0;

impl Emulator {
    pub fn new(mut audio_stream: AudioStream) -> Result<Self> {
        #[cfg(not(feature = "netplay"))]
        let mut nes_state = crate::emulation::LocalNesState::start_rom(
            &crate::bundle::Bundle::current().rom,
            true,
            Settings::current_mut().get_nes_region(),
        )?;

        #[cfg(feature = "netplay")]
        let mut nes_state = StateHandler::new()?;

        let (emulator_tx, emulator_rx) = tokio::sync::mpsc::channel(1);
        let (netplay_tx, netplay_rx) = tokio::sync::mpsc::channel(1);

        let shared_state = SharedState::new(emulator_tx.clone(), netplay_tx.clone());

        let inputs = Arc::new([AtomicU8::new(0), AtomicU8::new(0)]);
        let frame_buffer = VideoBufferPool::new();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();

        rt.spawn({
            let shared_emulator_state = shared_state.emulator_state.clone();
            let inputs = inputs.clone();
            let frame_buffer = frame_buffer.clone();
            let mut tx = audio_stream.tx.take();
            let mut emulator_rx = emulator_rx;
            let mut netplay_rx = netplay_rx;
            async move {
                loop {
                    // drain pending emulator commands
                    while let Ok(cmd) = emulator_rx.try_recv() {
                        match cmd {
                            EmulatorCommand::Reset(hard) => nes_state.reset(hard),
                            EmulatorCommand::SetSpeed(speed) => nes_state.set_speed(speed),
                        }
                    }

                    // drain pending netplay commands

                    while let Ok(cmd) = netplay_rx.try_recv() {
                        match cmd {
                            NetplayCommand::JoinGame(room_name) => nes_state.join_game(room_name),
                            NetplayCommand::FindGame => nes_state.find_game(),
                            NetplayCommand::HostGame => nes_state.host_game(),
                            NetplayCommand::CancelConnect => nes_state.cancel_connect(),
                            NetplayCommand::RetryConnect(start_method) => {
                                nes_state.retry_connect(start_method)
                            }
                            NetplayCommand::Resume => nes_state.resume(),
                            NetplayCommand::Disconnect => nes_state.disconnect(),
                        }
                    }

                    // main work: advance emulator when we have room
                    if let Some(ref mut prod) = tx {
                        if !prod.is_full() {
                            nes_state.advance(
                                [
                                    JoypadState(
                                        inputs[0].load(std::sync::atomic::Ordering::Relaxed),
                                    ),
                                    JoypadState(
                                        inputs[1].load(std::sync::atomic::Ordering::Relaxed),
                                    ),
                                ],
                                &mut NESBuffers {
                                    audio: Some(prod),
                                    video: frame_buffer.push_ref().as_deref_mut().ok(),
                                },
                            );
                            let frame = nes_state.frame();
                            shared_emulator_state.write().unwrap().frame = frame;
                            // 2) periodic SRAM snapshot (non-blocking check)
                            if frame % 100 == 0 {
                                println!("TICK");
                                use base64::Engine;
                                use base64::engine::general_purpose::STANDARD_NO_PAD as b64;
                                if let Some(sram) = nes_state.save_sram() {
                                    Settings::current_mut().save_state = Some(b64.encode(sram));
                                }
                            }
                        } else {
                            // back off a hair to avoid a busy loop when ring is full?
                            //tokio::task::yield_now().await;
                        }
                    }
                }
            }
        });

        Ok(Self {
            frame_buffer,
            shared_inputs: inputs,
            audio_stream,
            shared_state,
            rt,
        })
    }
}

pub trait NesStateHandler {
    fn advance(&mut self, joypad_state: [JoypadState; MAX_PLAYERS], buffers: &mut NESBuffers);
    fn reset(&mut self, hard: bool);
    fn set_speed(&mut self, speed: f32);
    fn save_sram(&self) -> Option<&[u8]>;
    #[cfg(feature = "debug")]
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
    pub audio: Option<&'a mut AudioProducer>,
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
