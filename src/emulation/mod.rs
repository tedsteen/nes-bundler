use std::{
    fmt::Debug,
    ops::{Deref, DerefMut},
    sync::{
        Arc,
        atomic::{AtomicU8, AtomicU32},
    },
    time::Instant,
};

use ringbuf::traits::Observer;
use serde::{Deserialize, Serialize};

use thingbuf::{Recycle, ThingBuf};
use tokio::sync::{
    mpsc::Sender,
    watch::{Receiver, channel},
};

use crate::{
    audio::{AudioProducer, AudioStream},
    emulation::tetanes::TetanesNesState,
    input::JoypadState,
    netplay::connection::ConnectingState,
    settings::{MAX_PLAYERS, Settings},
};

pub mod gui;
pub mod tetanes;
pub type LocalNesState = TetanesNesState;
pub fn new_local_nes_state() -> LocalNesState {
    LocalNesState::start_rom(
        &crate::bundle::Bundle::current().rom,
        true,
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

pub enum SharedNetplayConnectedState {
    Synchronizing,
    Running(Instant /* Start time */),
}

pub enum SharedNetplayState {
    Disconnected,
    Connecting(Receiver<ConnectingState>),
    Connected(SharedNetplayConnectedState),
    Resuming,
}

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

#[derive(Debug)]
pub enum NetplayCommand {
    JoinGame(String),
    FindGame,
    HostGame,

    CancelConnect,
    RetryConnect,

    Resume,
    Disconnect,
}
pub type NetplayCommandBus = Sender<NetplayCommand>;

pub struct SharedState {
    pub emulator_command_tx: EmulatorCommandBus,
    pub netplay_command_tx: NetplayCommandBus,

    pub netplay_receiver: Receiver<SharedNetplayState>,
    pub netplay_sender: tokio::sync::watch::Sender<SharedNetplayState>,

    pub emulator_state: Arc<SharedEmulatorState>,
}
impl SharedState {
    fn new(emulator_command_tx: EmulatorCommandBus, netplay_command_tx: NetplayCommandBus) -> Self {
        let (netplay_sender, netplay_receiver) = channel(SharedNetplayState::Disconnected);
        Self {
            emulator_command_tx: emulator_command_tx,
            netplay_command_tx: netplay_command_tx,

            netplay_receiver,
            netplay_sender,

            emulator_state: Arc::new(SharedEmulatorState::new()),
        }
    }
}

pub struct Emulator {
    pub frame_buffer: VideoBufferPool,
    pub shared_inputs: Arc<[AtomicU8; 2]>,

    pub shared_state: SharedState,

    pub audio_stream: AudioStream,
    th: std::thread::JoinHandle<()>,
}

pub const DEFAULT_SAMPLE_RATE: f32 = 44_100.0;

impl Emulator {
    pub fn new(mut audio_stream: AudioStream) -> Self {
        let nes_state = new_local_nes_state();
        if let Some(mut audio_producer) = audio_stream.tx.take() {
            let (emulator_tx, emulator_rx) = tokio::sync::mpsc::channel(1);
            let (netplay_tx, mut netplay_rx) = tokio::sync::mpsc::channel(1);

            let shared_state = SharedState::new(emulator_tx.clone(), netplay_tx.clone());

            let inputs = Arc::new([AtomicU8::new(0), AtomicU8::new(0)]);
            let frame_buffer = VideoBufferPool::new();
            let netplay_state_sender = shared_state.netplay_sender.clone();

            let th = std::thread::spawn({
                let shared_emulator_state = shared_state.emulator_state.clone();

                let inputs = inputs.clone();
                let frame_buffer = frame_buffer.clone();

                let mut emulator_rx = emulator_rx;
                || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();

                    let local = tokio::task::LocalSet::new();
                    local.block_on(&rt, async move {
                        #[cfg(feature = "netplay")]
                        let mut nes_state =
                            crate::netplay::Netplay::new(nes_state, netplay_state_sender);

                        loop {
                            // drain pending emulator commands
                            while let Ok(cmd) = emulator_rx.try_recv() {
                                match cmd {
                                    EmulatorCommand::Reset(hard) => nes_state.reset(hard),
                                    EmulatorCommand::SetSpeed(speed) => nes_state.set_speed(speed),
                                }
                            }
                            // drain pending netplay commands
                            #[cfg(feature = "netplay")]
                            while let Ok(cmd) = netplay_rx.try_recv() {
                                match cmd {
                                    NetplayCommand::FindGame => {
                                        nes_state.find_game().await;
                                    }

                                    NetplayCommand::HostGame => {
                                        nes_state.host_game().await;
                                    }
                                    NetplayCommand::JoinGame(room_name) => {
                                        nes_state.join_game(&room_name).await;
                                    }

                                    NetplayCommand::CancelConnect => {
                                        nes_state.cancel_connect();
                                    }
                                    NetplayCommand::RetryConnect => {
                                        nes_state.retry_connect();
                                    }
                                    NetplayCommand::Resume => {
                                        nes_state.resume();
                                    }
                                    NetplayCommand::Disconnect => {
                                        nes_state.disconnect();
                                    }
                                }
                            }
                            // main work: advance emulator when we have room
                            //Keep 1 frame of audio buffer
                            if audio_producer.occupied_len()
                                <= nes_state.get_samples_per_frame() as usize
                            {
                                let joypad_state = [
                                    JoypadState(
                                        inputs[0].load(std::sync::atomic::Ordering::Relaxed),
                                    ),
                                    JoypadState(
                                        inputs[1].load(std::sync::atomic::Ordering::Relaxed),
                                    ),
                                ];
                                let mut push_ref = frame_buffer.push_ref();
                                let buffers = &mut NESBuffers {
                                    audio: Some(&mut audio_producer),
                                    video: push_ref.as_deref_mut().ok(),
                                };
                                nes_state.advance(joypad_state, buffers).await;

                                let frame = nes_state.frame();
                                shared_emulator_state
                                    .frame
                                    .store(frame, std::sync::atomic::Ordering::Relaxed);

                                // 2) periodic SRAM snapshot (non-blocking check)
                                if frame % 100 == 0 {
                                    use base64::Engine;
                                    use base64::engine::general_purpose::STANDARD_NO_PAD as b64;
                                    if let Some(sram) = nes_state.save_sram() {
                                        let sram = sram.to_vec();
                                        // Do this in a blocking task as we want the main loop free from blocking code
                                        tokio::task::spawn_blocking(move || {
                                            Settings::current_mut().save_state =
                                                Some(b64.encode(sram));
                                        });
                                    }
                                }
                            } else {
                                // back off a hair to avoid a busy loop when ring is full
                                tokio::task::yield_now().await;
                            }
                        }
                    });
                }
            });
            Self {
                frame_buffer,
                shared_inputs: inputs,
                audio_stream,
                shared_state,
                th,
            }
        } else {
            panic!("No audio producer")
        }
    }
}

pub trait NesStateHandler {
    async fn advance(&mut self, joypad_state: [JoypadState; MAX_PLAYERS], buffers: &mut NESBuffers);
    fn reset(&mut self, hard: bool);
    fn set_speed(&mut self, speed: f32);
    fn save_sram(&self) -> Option<&[u8]>;
    #[cfg(feature = "debug")]
    fn frame(&self) -> u32;
    fn get_samples_per_frame(&self) -> f32;
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
