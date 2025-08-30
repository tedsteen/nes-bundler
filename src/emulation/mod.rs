use std::{
    ops::{Deref, DerefMut},
    sync::{
        Arc, Mutex,
        atomic::AtomicU8,
        mpsc::{Sender, channel},
    },
    time::Duration,
};

use anyhow::Result;

use ringbuf::traits::Observer;
use serde::{Deserialize, Serialize};

use thingbuf::{Recycle, ThingBuf};

use crate::{
    audio::{AudioProducer, AudioStream},
    input::JoypadState,
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
pub enum EmulatorCommand {
    Reset(bool),
    SetSpeed(f32),
}

pub struct Emulator {
    pub command_tx: Sender<EmulatorCommand>,
    pub frame_buffer: VideoBufferPool,
    pub shared_inputs: Arc<[AtomicU8; 2]>,
    pub nes_state: Arc<Mutex<crate::netplay::NetplayStateHandler>>,
    pub audio_stream: AudioStream,
}

pub const SAMPLE_RATE: f32 = 44_100.0;

impl Emulator {
    pub async fn new(mut audio_stream: AudioStream) -> Result<Self> {
        #[cfg(not(feature = "netplay"))]
        let nes_state = crate::emulation::LocalNesState::start_rom(
            &crate::bundle::Bundle::current().rom,
            true,
            Settings::current_mut().get_nes_region(),
        )?;

        #[cfg(feature = "netplay")]
        let nes_state = crate::netplay::NetplayStateHandler::new()?;

        let nes_state = Arc::new(Mutex::new(nes_state));
        let (command_tx, command_rx) = channel();
        let inputs = Arc::new([AtomicU8::new(0), AtomicU8::new(0)]);
        let frame_buffer = VideoBufferPool::new();

        tokio::task::spawn({
            let nes_state = nes_state.clone();
            async move {
                loop {
                    for command in command_rx.iter() {
                        let mut nes_state = nes_state.lock().unwrap();
                        match command {
                            EmulatorCommand::Reset(hard) => nes_state.reset(hard),
                            EmulatorCommand::SetSpeed(speed) => nes_state.set_speed(speed),
                        }
                    }
                }
            }
        });
        tokio::task::spawn({
            let nes_state = nes_state.clone();
            async move {
                let mut ticker = tokio::time::interval(Duration::from_millis(500));
                loop {
                    ticker.tick().await;
                    use base64::Engine;
                    use base64::engine::general_purpose::STANDARD_NO_PAD as b64;
                    if let Some(sram) = nes_state.lock().unwrap().save_sram() {
                        Settings::current_mut().save_state = Some(b64.encode(sram));
                    }
                }
            }
        });

        tokio::task::spawn({
            let nes_state = nes_state.clone();
            let inputs = inputs.clone();
            let frame_buffer = frame_buffer.clone();
            let mut tx = audio_stream.tx.take();
            async move {
                loop {
                    let mut nes_state = nes_state.lock().unwrap();
                    if let Some(audio_producer) = &tx {
                        if audio_producer.is_full() {
                            //TODO: park the thread or something?
                            continue;
                        }
                    }

                    nes_state.advance(
                        [
                            JoypadState(inputs[0].load(std::sync::atomic::Ordering::Relaxed)),
                            JoypadState(inputs[1].load(std::sync::atomic::Ordering::Relaxed)),
                        ],
                        &mut NESBuffers {
                            audio: tx.as_mut(),
                            video: frame_buffer.push_ref().as_deref_mut().ok(),
                        },
                    );
                }
            }
        });

        Ok(Self {
            frame_buffer,
            command_tx,
            shared_inputs: inputs,
            nes_state,
            audio_stream,
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
