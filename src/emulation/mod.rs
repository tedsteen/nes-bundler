use std::{
    ops::{Deref, DerefMut},
    sync::{
        mpsc::{channel, Sender},
        Arc, Mutex, RwLock,
    },
};

use anyhow::Result;

use serde::{Deserialize, Serialize};

use thingbuf::{Recycle, ThingBuf};

use crate::{
    audio::AudioSender,
    fps::RateCounter,
    input::JoypadState,
    settings::{Settings, MAX_PLAYERS},
};

pub mod gui;
pub mod tetanes;
use self::{gui::EmulatorGui, tetanes::TetanesNesState};
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
pub struct Emulator {}
pub const SAMPLE_RATE: f32 = 44_100.0;

impl Emulator {
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }

    pub async fn start_thread(
        &self,
        audio_tx: AudioSender,
        inputs: Arc<RwLock<[JoypadState; MAX_PLAYERS]>>,
        frame_buffer: VideoBufferPool,
    ) -> Result<(EmulatorGui, Sender<EmulatorCommand>)> {
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
        let audio_buffer = AudioBufferPool::new();

        tokio::task::spawn({
            let nes_state = nes_state.clone();
            async move {
                let rate_counter = Arc::new(Mutex::new(RateCounter::new()));
                loop {
                    for command in command_rx.try_iter() {
                        let mut nes_state = nes_state.lock().unwrap();
                        match command {
                            EmulatorCommand::Reset(hard) => nes_state.reset(hard),
                            EmulatorCommand::SetSpeed(speed) => nes_state.set_speed(speed),
                        }
                    }

                    // Run advance in parallel with the audio pushing
                    let _ = tokio::join!(
                        tokio::spawn({
                            let audio_buffer = audio_buffer.clone();
                            let audio_tx = audio_tx.clone();
                            async move {
                                #[cfg(feature = "debug")]
                                puffin::profile_scope!("push audio");
                                audio_buffer.pop_with(|audio_buffer| {
                                    for s in audio_buffer.drain(..) {
                                        let _ = audio_tx.send(s);
                                    }
                                });
                            }
                        }),
                        tokio::spawn({
                            let frame_buffer = frame_buffer.clone();
                            let nes_state = nes_state.clone();
                            let joypad_state = *inputs.read().unwrap();
                            let audio_buffer = audio_buffer.clone();
                            let rate_counter = rate_counter.clone();
                            async move {
                                let _ = audio_buffer.push_with(|audio_buffer| {
                                    let frame = frame_buffer.push_ref();
                                    if frame.is_err() {
                                        //TODO: If we get in a bad sync with vsync and drop a lot of frames then perhaps we can do something to yank things in place again?
                                        rate_counter.lock().unwrap().tick("Dropped frame");
                                    } else {
                                        rate_counter.lock().unwrap().tick("Frame");
                                    }
                                    log::trace!("Advance NES with joypad state {:?}", joypad_state);
                                    nes_state.lock().unwrap().advance(
                                        joypad_state,
                                        &mut NESBuffers {
                                            video: frame.ok().as_deref_mut(),
                                            audio: Some(audio_buffer),
                                        },
                                    );
                                });
                                if let Some(report) = rate_counter.lock().unwrap().report() {
                                    // Hitch-hike on the once-per-second-reporting to save the sram.
                                    use base64::engine::general_purpose::STANDARD_NO_PAD as b64;
                                    use base64::Engine;
                                    Settings::current_mut().save_state = nes_state
                                        .lock()
                                        .unwrap()
                                        .save_sram()
                                        .map(|sram| b64.encode(sram));

                                    log::debug!("Emulation: {report}");
                                }
                            }
                        })
                    );
                }
            }
        });
        Ok((EmulatorGui::new(nes_state, command_tx.clone()), command_tx))
    }
}

pub trait NesStateHandler {
    fn advance(&mut self, joypad_state: [JoypadState; MAX_PLAYERS], buffers: &mut NESBuffers);
    fn reset(&mut self, hard: bool);
    fn set_speed(&mut self, speed: f32);
    fn save_sram(&self) -> Option<&[u8]>;
    #[cfg(feature = "netplay")]
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
    pub audio: Option<&'a mut NESAudioFrame>,
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

pub struct NESAudioFrame(Vec<f32>);
impl NESAudioFrame {
    pub fn new() -> NESAudioFrame {
        Self(Vec::new())
    }
}

impl Deref for NESAudioFrame {
    type Target = Vec<f32>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NESAudioFrame {
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

impl Recycle<NESAudioFrame> for FrameRecycle {
    fn new_element(&self) -> NESAudioFrame {
        NESAudioFrame::new()
    }

    fn recycle(&self, _frame: &mut NESAudioFrame) {}
}

#[derive(Debug)]
pub struct AudioBufferPool(Arc<ThingBuf<NESAudioFrame, FrameRecycle>>);

impl AudioBufferPool {
    pub fn new() -> Self {
        Self(Arc::new(ThingBuf::with_recycle(2, FrameRecycle)))
    }
}

impl Deref for AudioBufferPool {
    type Target = Arc<ThingBuf<NESAudioFrame, FrameRecycle>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Clone for AudioBufferPool {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}
