use super::StateHandler;
use crate::settings::gui::GuiComponent;

#[cfg(feature = "debug")]
struct DebugGui {
    pub speed: f32,
    pub override_speed: bool,
}

pub struct EmulatorGui {
    pub nes_state: StateHandler,
    #[cfg(feature = "netplay")]
    pub netplay_gui: crate::netplay::gui::NetplayGui,
    #[cfg(feature = "debug")]
    debug_gui: DebugGui,
}
impl EmulatorGui {
    pub fn new(nes_state: StateHandler) -> Self {
        Self {
            nes_state,
            #[cfg(feature = "netplay")]
            netplay_gui: crate::netplay::gui::NetplayGui::new(),
            #[cfg(feature = "debug")]
            debug_gui: DebugGui {
                speed: 1.0,
                override_speed: false,
            },
        }
    }
}
#[cfg(feature = "debug")]
impl DebugGui {
    fn ui(&mut self, ui: &mut egui::Ui, nes_state: &StateHandler) {
        ui.label(format!(
            "Frame: {}",
            super::NesStateHandler::frame(nes_state)
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
                        && !self.override_speed
                    {
                        *super::Emulator::emulation_speed_mut() = 1.0;
                    }

                    if self.override_speed {
                        ui.add(
                            egui::Slider::new(&mut self.speed, 0.01..=1.0)
                                .suffix("x")
                                .logarithmic(true),
                        );
                        *super::Emulator::emulation_speed_mut() = self.speed;
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
        self.debug_gui.ui(ui, &self.nes_state);

        #[cfg(feature = "netplay")]
        self.netplay_gui.ui(ui, &mut self.nes_state);
    }

    #[cfg(feature = "netplay")]
    fn messages(&self) -> Option<Vec<String>> {
        self.netplay_gui.messages(&self.nes_state)
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

    #[cfg(all(feature = "netplay", feature = "debug"))]
    fn prepare(&mut self) {
        self.netplay_gui.prepare(&self.nes_state);
    }
}
