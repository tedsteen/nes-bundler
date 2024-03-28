use std::sync::{Arc, Mutex};

use crate::bundle::Bundle;
use crate::window::NESFramePool;
use crate::{
    audio::Audio,
    fps::RateCounter,
    input::{Inputs, JoypadState},
    settings::{gui::GuiComponent, MAX_PLAYERS},
};
use anyhow::Result;

use crate::nes_state::NesStateHandler;

use super::FrameData;

#[cfg(feature = "netplay")]
type StateHandler = crate::netplay::NetplayStateHandler;
#[cfg(not(feature = "netplay"))]
type StateHandler = crate::nes_state::LocalNesState;

pub struct Emulator {
    pub frame_pool: NESFramePool,
    nes_state: Arc<Mutex<StateHandler>>,
    joypads: Arc<Mutex<[JoypadState; MAX_PLAYERS]>>,
}
impl Emulator {
    pub fn start(inputs: &Inputs, audio: &mut Audio) -> Result<Self> {
        #[cfg(not(feature = "netplay"))]
        let nes_state = crate::nes_state::LocalNesState::start_rom(&Bundle::current().rom)?;

        #[cfg(feature = "netplay")]
        let nes_state = crate::netplay::NetplayStateHandler::new()?;

        let audio_tx = audio.stream.start()?;

        let frame_pool = NESFramePool::new();
        let shared_state = Emulator {
            frame_pool: frame_pool.clone(),
            nes_state: Arc::new(Mutex::new(nes_state)),
            joypads: inputs.joypads.clone(),
        };

        tokio::spawn({
            let nes_state = shared_state.nes_state.clone();
            let joypads = shared_state.joypads.clone();
            async move {
                let mut loop_counter = RateCounter::new();
                loop {
                    loop_counter.tick("Frames");

                    let joypads = *joypads.lock().unwrap();
                    let push_result = frame_pool.push_with(|nes_frame| {
                        nes_state
                            .lock()
                            .unwrap()
                            .advance(joypads, &mut Some(nes_frame))
                    });

                    if let Some(FrameData { audio }) = match push_result {
                        Ok(audio) => audio,
                        Err(_) => {
                            loop_counter.tick("Dropped Frames");
                            nes_state.lock().unwrap().advance(joypads, &mut None)
                        }
                    } {
                        log::trace!("Pushing {:} audio samples", audio.len());
                        for s in audio {
                            let _ = audio_tx.send(s).await;
                        }
                    } else {
                        log::trace!("No frame, pushing silence to keep frame count down");
                        for _ in 0..200 {
                            let _ = audio_tx.send(0.0).await;
                        }
                    }

                    if let Some(report) = loop_counter.report() {
                        log::debug!("{report}");
                    }
                }
            }
        });
        Ok(shared_state)
    }

    pub fn save_state(&self) -> Option<Vec<u8>> {
        self.nes_state.lock().unwrap().save()
    }

    pub fn load_state(&mut self, data: &mut Vec<u8>) {
        self.nes_state.lock().unwrap().load(data);
    }
}

#[cfg(feature = "debug")]
pub struct DebugGui {
    speed: f32,
    override_speed: bool,
}

pub struct EmulatorGui {
    #[cfg(feature = "netplay")]
    netplay_gui: crate::netplay::gui::NetplayGui,
    #[cfg(feature = "debug")]
    debug_gui: DebugGui,
}

#[cfg(feature = "debug")]
impl GuiComponent<Emulator> for DebugGui {
    fn ui(&mut self, instance: &mut Emulator, ui: &mut egui::Ui) {
        ui.label(format!(
            "Frame: {}",
            instance.nes_state.lock().unwrap().frame()
        ));
        ui.horizontal(|ui| {
            egui::Grid::new("debug_grid")
                .num_columns(2)
                .spacing([10.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    if ui
                        .checkbox(&mut self.override_speed, "Override emulation speed")
                        .changed()
                    {
                        let speed = if self.override_speed { self.speed } else { 1.0 };
                        instance.nes_state.lock().unwrap().set_speed(speed);
                    }
                    if self.override_speed {
                        let speed_changed = ui
                            .add(egui::Slider::new(&mut self.speed, 0.001..=2.5).suffix("x"))
                            .changed();
                        if speed_changed {
                            instance.nes_state.lock().unwrap().set_speed(self.speed);
                        };
                    }
                    ui.end_row();
                });
        });
    }
}

impl EmulatorGui {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "netplay")]
            netplay_gui: crate::netplay::gui::NetplayGui::new(
                Bundle::current().config.netplay.clone(),
            ),
            #[cfg(feature = "debug")]
            debug_gui: DebugGui {
                speed: 1.0,
                override_speed: false,
            },
        }
    }
}

impl GuiComponent<Emulator> for EmulatorGui {
    #[allow(unused_variables)]
    fn ui(&mut self, instance: &mut Emulator, ui: &mut egui::Ui) {
        #[cfg(feature = "debug")]
        self.debug_gui.ui(instance, ui);

        #[cfg(feature = "netplay")]
        self.netplay_gui
            .ui(&mut instance.nes_state.lock().unwrap(), ui);
    }

    #[cfg(feature = "netplay")]
    fn messages(&self, instance: &Emulator) -> Option<Vec<String>> {
        self.netplay_gui
            .messages(&instance.nes_state.lock().unwrap())
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
    fn prepare(&mut self, instance: &mut Emulator) {
        self.netplay_gui
            .prepare(&mut instance.nes_state.lock().unwrap());
    }
}
