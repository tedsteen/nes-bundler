use crate::bundle::Bundle;
use crate::netplay::NetplayStateHandler;
use crate::settings::gui::GuiComponent;
use anyhow::Result;

use crate::nes_state::NesStateHandler;

#[cfg(feature = "netplay")]
type StateHandler = crate::netplay::NetplayStateHandler;
#[cfg(not(feature = "netplay"))]
type StateHandler = crate::nes_state::LocalNesState;

pub struct Emulator {
    pub nes_state: StateHandler,
}
impl Emulator {
    pub fn start() -> Result<Self> {
        #[cfg(not(feature = "netplay"))]
        let nes_state = crate::nes_state::LocalNesState::start_rom(&Bundle::current().rom)?;

        #[cfg(feature = "netplay")]
        let nes_state = crate::netplay::NetplayStateHandler::new()?;

        let shared_state = Emulator { nes_state };

        Ok(shared_state)
    }

    pub fn save_state(&self) -> Option<Vec<u8>> {
        self.nes_state.save()
    }

    pub fn load_state(&mut self, data: &mut Vec<u8>) {
        self.nes_state.load(data);
    }

    pub fn get_emulation_speed(&self) -> f32 {
        *NetplayStateHandler::emulation_speed().lock().unwrap()
    }
}

#[cfg(feature = "debug")]
pub struct DebugGui {
    pub speed: f32,
    pub override_speed: bool,
}

pub struct EmulatorGui {
    #[cfg(feature = "netplay")]
    netplay_gui: crate::netplay::gui::NetplayGui,
    #[cfg(feature = "debug")]
    pub debug_gui: DebugGui,
}

#[cfg(feature = "debug")]
impl GuiComponent<Emulator> for DebugGui {
    fn ui(&mut self, instance: &mut Emulator, ui: &mut egui::Ui) {
        ui.label(format!("Frame: {}", instance.nes_state.frame()));
        ui.horizontal(|ui| {
            egui::Grid::new("debug_grid")
                .num_columns(2)
                .spacing([10.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.checkbox(&mut self.override_speed, "Override emulation speed");

                    if self.override_speed {
                        ui.add(egui::Slider::new(&mut self.speed, 0.01..=4.0).suffix("x"));
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
        self.netplay_gui.ui(&mut instance.nes_state, ui);
    }

    #[cfg(feature = "netplay")]
    fn messages(&self, instance: &Emulator) -> Option<Vec<String>> {
        self.netplay_gui.messages(&instance.nes_state)
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
        self.netplay_gui.prepare(&mut instance.nes_state);
    }
}
