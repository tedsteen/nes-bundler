#[cfg(feature = "debug")]
use crate::emulation::SharedEmulator;
use crate::{emulation::SharedState, main_view::gui::GuiComponent};

#[cfg(feature = "debug")]
struct DebugGui {
    pub speed: f32,
    pub override_speed: bool,
    shared_emulator: SharedEmulator,
}

pub struct EmulatorGui {
    #[cfg(feature = "netplay")]
    pub netplay_gui: crate::netplay::gui::NetplayGui,
    #[cfg(feature = "debug")]
    debug_gui: DebugGui,
}
impl EmulatorGui {
    #[allow(unused_variables)]
    pub fn new(shared_state: SharedState) -> Self {
        Self {
            #[cfg(feature = "netplay")]
            netplay_gui: crate::netplay::gui::NetplayGui::new(shared_state.netplay.clone()),
            #[cfg(feature = "debug")]
            debug_gui: DebugGui {
                speed: 1.0,
                override_speed: false,
                shared_emulator: shared_state.emulator.clone(),
            },
        }
    }
}
#[cfg(feature = "debug")]
impl DebugGui {
    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.end_row();

        ui.label(format!(
            "Frame: {}",
            self.shared_emulator
                .state
                .frame
                .load(std::sync::atomic::Ordering::Relaxed)
        ));
        ui.end_row();

        if ui
            .checkbox(&mut self.override_speed, "Override emulation speed")
            .changed()
            && !self.override_speed
        {
            self.speed = 1.0;
            let _ = self
                .shared_emulator
                .command_tx
                .try_send(crate::emulation::EmulatorCommand::SetSpeed(1.0));
        }

        if self.override_speed {
            ui.end_row();
            if ui
                .add(egui::Slider::new(&mut self.speed, 0.01..=2.0).suffix("x"))
                .changed()
            {
                let _ = self
                    .shared_emulator
                    .command_tx
                    .try_send(crate::emulation::EmulatorCommand::SetSpeed(self.speed));
            }
        }
        ui.end_row();
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
