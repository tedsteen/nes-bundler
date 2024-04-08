use std::{
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex, OnceLock, RwLock, RwLockReadGuard},
};

use anyhow::Result;

use serde::Deserialize;

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
        frame_buffer: BufferPool,
    ) -> Result<EmulatorGui> {
        #[cfg(not(feature = "netplay"))]
        let nes_state = crate::emulation::LocalNesState::start_rom(
            &crate::bundle::Bundle::current().rom,
            true,
        )?;

        #[cfg(feature = "netplay")]
        let nes_state = crate::netplay::NetplayStateHandler::new()?;

        let nes_state = Arc::new(Mutex::new(nes_state));

        tokio::task::spawn_blocking({
            let nes_state = nes_state.clone();
            move || {
                let mut audio_buffer = NESAudioFrame::new();

                let mut rate_counter = RateCounter::new();
                loop {
                    #[cfg(feature = "debug")]
                    puffin::profile_function!("Emulator loop");

                    {
                        #[cfg(feature = "debug")]
                        puffin::profile_scope!("push audio");

                        log::trace!("Pushing {:} audio samples", audio_buffer.len());
                        for s in audio_buffer.iter() {
                            let _ = audio_tx.send(*s);
                        }
                    }

                    {
                        #[cfg(feature = "debug")]
                        puffin::profile_scope!("advance");

                        rate_counter.tick("Frame");
                        audio_buffer.clear();
                        let frame = frame_buffer.push_ref();
                        if frame.is_err() {
                            //TODO: If we get in a bad sync with vsync and drop a lot of frames then perhaps we can do something to yank things in place again?
                            rate_counter.tick("Dropped frame");
                        }
                        nes_state.lock().unwrap().advance(
                            *inputs.read().unwrap(),
                            &mut NESBuffers {
                                video: frame.ok().as_deref_mut(),
                                audio: Some(&mut audio_buffer),
                            },
                        );
                    }

                    if let Some(report) = rate_counter.report() {
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
            }
        });
        Ok(EmulatorGui::new(nes_state))
    }

    fn _emulation_speed() -> &'static RwLock<f32> {
        static MEM: OnceLock<RwLock<f32>> = OnceLock::new();
        MEM.get_or_init(|| RwLock::new(1_f32))
    }

    pub fn emulation_speed<'a>() -> RwLockReadGuard<'a, f32> {
        Self::_emulation_speed().read().unwrap()
    }

    #[cfg(any(feature = "netplay", feature = "debug"))]
    pub fn emulation_speed_mut<'a>() -> std::sync::RwLockWriteGuard<'a, f32> {
        Self::_emulation_speed().write().unwrap()
    }
}

pub trait NesStateHandler {
    fn advance(&mut self, joypad_state: [JoypadState; MAX_PLAYERS], buffers: &mut NESBuffers);
    fn save_sram(&self) -> Option<&[u8]>;
    fn frame(&self) -> u32;
}

#[derive(Deserialize, Debug)]
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
pub struct BufferPool(Arc<ThingBuf<NESVideoFrame, FrameRecycle>>);

impl BufferPool {
    pub fn new() -> Self {
        Self(Arc::new(ThingBuf::with_recycle(1, FrameRecycle)))
    }
}

impl Deref for BufferPool {
    type Target = Arc<ThingBuf<NESVideoFrame, FrameRecycle>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Clone for BufferPool {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}
