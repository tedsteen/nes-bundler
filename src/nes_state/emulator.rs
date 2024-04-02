use std::{
    ops::Deref,
    sync::{Arc, Mutex, OnceLock, RwLock},
};

use crate::{
    audio::AudioSender,
    fps::RateCounter,
    input::JoypadState,
    settings::{gui::GuiComponent, Settings, MAX_PLAYERS},
};
use anyhow::Result;
use thingbuf::{Recycle, ThingBuf};

use super::{NESAudioFrame, NESBuffers, NESVideoFrame};
use crate::nes_state::NesStateHandler;

#[cfg(feature = "netplay")]
type StateHandler = crate::netplay::NetplayStateHandler;
#[cfg(not(feature = "netplay"))]
type StateHandler = crate::nes_state::LocalNesState;

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

                    let push_frame_res = frame_pool.push_with(|video_buffer| {
                        rate_counter.tick("Frame");
                        nes_state.lock().unwrap().advance(
                            *joypads.read().unwrap(),
                            &mut NESBuffers {
                                video: Some(video_buffer),
                                audio: Some(&mut audio_buffer),
                            },
                        );
                    });
                    if push_frame_res.is_err() {
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

    pub fn emulation_speed() -> &'static RwLock<f32> {
        static MEM: OnceLock<RwLock<f32>> = OnceLock::new();
        MEM.get_or_init(|| RwLock::new(1_f32))
    }
}

#[cfg(feature = "debug")]
pub struct DebugGui {
    nes_state: Arc<Mutex<StateHandler>>,

    pub speed: f32,
    pub override_speed: bool,
}

pub struct EmulatorGui {
    #[cfg(feature = "netplay")]
    netplay_gui: crate::netplay::gui::NetplayGui,
    #[cfg(feature = "debug")]
    pub debug_gui: DebugGui,
}
impl EmulatorGui {
    pub fn new(nes_state: Arc<Mutex<StateHandler>>) -> Self {
        Self {
            #[cfg(feature = "netplay")]
            netplay_gui: crate::netplay::gui::NetplayGui::new(nes_state.clone()),
            #[cfg(feature = "debug")]
            debug_gui: DebugGui {
                speed: 1.0,
                override_speed: false,
                nes_state,
            },
        }
    }
}
#[cfg(feature = "debug")]
impl GuiComponent for DebugGui {
    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.label(format!("Frame: {}", self.nes_state.lock().unwrap().frame()));
        ui.horizontal(|ui| {
            egui::Grid::new("debug_grid")
                .num_columns(2)
                .spacing([10.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    if ui
                        .checkbox(&mut self.override_speed, "Override emulation speed")
                        .changed()
                        && !self.override_speed
                    {
                        *Emulator::emulation_speed().write().unwrap() = 1.0;
                    }

                    if self.override_speed {
                        ui.add(egui::Slider::new(&mut self.speed, 0.01..=2.0).suffix("x"));
                        *Emulator::emulation_speed().write().unwrap() = self.speed;
                    }
                    ui.end_row();
                });
        });
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

impl Default for BufferPool {
    fn default() -> Self {
        Self::new()
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

impl GuiComponent for EmulatorGui {
    #[allow(unused_variables)]
    fn ui(&mut self, ui: &mut egui::Ui) {
        #[cfg(feature = "debug")]
        self.debug_gui.ui(ui);

        #[cfg(feature = "netplay")]
        self.netplay_gui.ui(ui);
    }

    #[cfg(feature = "netplay")]
    fn messages(&self) -> Option<Vec<String>> {
        self.netplay_gui.messages()
    }

    fn name(&self) -> Option<String> {
        if cfg!(feature = "netplay") {
            #[cfg(feature = "netplay")]
            return self.netplay_gui.name();
        } else if cfg!(feature = "debug") {
            return Some("Debug".to_string());
        }

        None
    }

    #[cfg(feature = "netplay")]
    fn prepare(&mut self) {
        self.netplay_gui.prepare();
    }
}
