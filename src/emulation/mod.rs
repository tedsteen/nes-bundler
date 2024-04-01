use std::{
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use anyhow::Result;
use serde::Deserialize;
use thingbuf::Recycle;
use tokio::task::JoinHandle;

use crate::{
    audio::AudioSender,
    fps::RateCounter,
    input::JoypadState,
    main_view::BufferPool,
    settings::{Settings, MAX_PLAYERS},
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

pub struct Emulator {
    pub nes_state: Arc<Mutex<StateHandler>>,
}
pub const SAMPLE_RATE: f32 = 44_100.0;

impl Emulator {
    pub fn new() -> Result<Self> {
        #[cfg(not(feature = "netplay"))]
        let nes_state = crate::emulation::LocalNesState::start_rom(
            &crate::bundle::Bundle::current().rom,
            true,
        )?;

        #[cfg(feature = "netplay")]
        let nes_state = crate::netplay::NetplayStateHandler::new()?;

        Ok(Self {
            nes_state: Arc::new(Mutex::new(nes_state)),
        })
    }
    pub fn start_thread(
        &self,
        frame_pool: BufferPool,
        audio_tx: AudioSender,
        joypads: Arc<RwLock<[JoypadState; MAX_PLAYERS]>>,
    ) -> JoinHandle<()> {
        let audio_tx = audio_tx.clone();
        let frame_pool = frame_pool.clone();
        let joypads = joypads.clone();
        let nes_state = self.nes_state.clone();
        tokio::task::spawn_blocking(move || {
            let mut audio_buffer = NESAudioFrame::new();
            let mut rate_counter = RateCounter::new();
            loop {
                #[cfg(feature = "debug")]
                puffin::profile_function!("Emulator loop");
                audio_buffer.clear();
                {
                    #[cfg(feature = "debug")]
                    puffin::profile_scope!("advance");

                    let frame_pool_full = frame_pool
                        .push_with(|video_buffer| {
                            rate_counter.tick("Frame");
                            nes_state.lock().unwrap().advance(
                                *joypads.read().unwrap(),
                                &mut NESBuffers {
                                    video: Some(video_buffer),
                                    audio: Some(&mut audio_buffer),
                                },
                            );
                        })
                        .is_err();
                    if frame_pool_full {
                        rate_counter.tick("Dropped Frame");
                        nes_state.lock().unwrap().advance(
                            *joypads.read().unwrap(),
                            &mut NESBuffers {
                                video: None,
                                audio: Some(&mut audio_buffer),
                            },
                        );
                    };
                }
                #[cfg(feature = "debug")]
                puffin::profile_scope!("push audio");
                log::trace!("Pushing {:} audio samples", audio_buffer.len());
                for s in audio_buffer.iter() {
                    let _ = audio_tx.send(*s);
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
        })
    }

    fn _emulation_speed() -> &'static RwLock<f32> {
        static MEM: OnceLock<RwLock<f32>> = OnceLock::new();
        MEM.get_or_init(|| RwLock::new(1_f32))
    }

    pub fn emulation_speed<'a>() -> RwLockReadGuard<'a, f32> {
        Self::_emulation_speed().read().unwrap()
    }

    pub fn emulation_speed_mut<'a>() -> RwLockWriteGuard<'a, f32> {
        Self::_emulation_speed().write().unwrap()
    }
}

pub trait NesStateHandler {
    fn advance(&mut self, joypad_state: [JoypadState; MAX_PLAYERS], buffers: &mut NESBuffers);
    fn save_sram(&self) -> Option<Vec<u8>>;
    fn load_sram(&mut self, data: &mut Vec<u8>);
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
    fn new() -> NESAudioFrame {
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
