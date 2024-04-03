use crate::emulation::NesStateHandler;
use crate::settings::gui::GuiComponent;
use std::sync::{Arc, Mutex};

use super::{Emulator, StateHandler};

#[cfg(feature = "debug")]
struct DebugGui {
    nes_state: Arc<Mutex<StateHandler>>,

    pub speed: f32,
    pub override_speed: bool,
}

pub struct EmulatorGui {
    #[cfg(feature = "netplay")]
    netplay_gui: crate::netplay::gui::NetplayGui,
    #[cfg(feature = "debug")]
    debug_gui: DebugGui,
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
                        *Emulator::emulation_speed_mut() = 1.0;
                    }

                    if self.override_speed {
                        ui.add(egui::Slider::new(&mut self.speed, 0.01..=2.0).suffix("x"));
                        *Emulator::emulation_speed_mut() = self.speed;
                    }
                    ui.end_row();
                });
        });
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
