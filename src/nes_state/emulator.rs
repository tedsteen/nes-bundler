use std::sync::{Arc, Mutex, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::{
    audio::AudioSender,
    fps::RateCounter,
    input::JoypadState,
    main_view::BufferPool,
    settings::{Settings, MAX_PLAYERS},
};
use anyhow::Result;

use super::{NESAudioFrame, NESBuffers};
use crate::nes_state::NesStateHandler;

#[cfg(feature = "netplay")]
pub type StateHandler = crate::netplay::NetplayStateHandler;
#[cfg(not(feature = "netplay"))]
pub type StateHandler = crate::nes_state::LocalNesState;

pub struct Emulator {
    pub nes_state: Arc<Mutex<StateHandler>>,
}
pub const SAMPLE_RATE: f32 = 44_100.0;

impl Emulator {
    pub fn new() -> Result<Self> {
        #[cfg(not(feature = "netplay"))]
        let nes_state = crate::nes_state::LocalNesState::start_rom(
            &crate::bundle::Bundle::current().rom,
            true,
        )?;

        #[cfg(feature = "netplay")]
        let nes_state = crate::netplay::NetplayStateHandler::new()?;

        Ok(Self {
            nes_state: Arc::new(Mutex::new(nes_state)),
        })
    }
    pub fn start(
        &self,
        frame_pool: BufferPool,
        audio_tx: AudioSender,
        joypads: Arc<RwLock<[JoypadState; MAX_PLAYERS]>>,
    ) -> Result<()> {
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
        });

        Ok(())
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
