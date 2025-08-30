use crate::{emulation::Emulator, main_view::gui::GuiComponent};

use super::EmulatorCommand;

#[cfg(feature = "debug")]
struct DebugGui {
    pub speed: f32,
    pub override_speed: bool,
}

pub struct EmulatorGui {
    #[cfg(feature = "netplay")]
    pub netplay_gui: crate::netplay::gui::NetplayGui,
    #[cfg(feature = "debug")]
    debug_gui: DebugGui,
}
impl EmulatorGui {
    #[allow(unused_variables)]
    pub fn new() -> Self {
        Self {
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
    fn ui(&mut self, ui: &mut egui::Ui, emulator: &mut Emulator) {
        ui.end_row();

        ui.label(format!(
            "Frame: {}",
            emulator.shared_state.emulator_state.read().unwrap().frame
        ));
        ui.end_row();

        if ui
            .checkbox(&mut self.override_speed, "Override emulation speed")
            .changed()
            && !self.override_speed
        {
            let _ = emulator
                .shared_state
                .emulator_command_tx
                .try_send(EmulatorCommand::SetSpeed(1.0));
        }

        if self.override_speed {
            ui.end_row();
            ui.add(egui::Slider::new(&mut self.speed, 0.005..=2.0).suffix("x"));
            let _ = emulator
                .shared_state
                .emulator_command_tx
                .try_send(EmulatorCommand::SetSpeed(self.speed));
        }
        ui.end_row();
    }
}

impl GuiComponent for EmulatorGui {
    #[allow(unused_variables)]
    fn ui(&mut self, ui: &mut egui::Ui, emulator: &mut Emulator) {
        #[cfg(feature = "debug")]
        self.debug_gui.ui(ui, emulator);

        #[cfg(feature = "netplay")]
        self.netplay_gui.ui(ui, emulator);
    }

    #[cfg(feature = "netplay")]
    fn messages(&self, emulator: &Emulator) -> Option<Vec<String>> {
        self.netplay_gui.messages(emulator)
    }

    fn name(&self) -> Option<&str> {
        if cfg!(feature = "netplay") {
            #[cfg(feature = "netplay")]
            return self.netplay_gui.name();
        } else if cfg!(feature = "debug") {
            return Some("Debug");
        }

        None
    }
}
