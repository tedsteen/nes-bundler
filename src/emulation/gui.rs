use std::sync::{mpsc::Sender, Arc, Mutex};

use crate::main_view::gui::GuiComponent;

use super::{EmulatorCommand, StateHandler};

#[cfg(feature = "debug")]
struct DebugGui {
    nes_state: Arc<Mutex<StateHandler>>,
    emulator_tx: Sender<EmulatorCommand>,

    pub speed: f32,
    pub override_speed: bool,
}

pub struct EmulatorGui {
    #[cfg(feature = "netplay")]
    nes_state: Arc<Mutex<StateHandler>>,

    #[cfg(feature = "netplay")]
    pub netplay_gui: crate::netplay::gui::NetplayGui,
    #[cfg(feature = "debug")]
    debug_gui: DebugGui,
}
impl EmulatorGui {
    #[allow(unused_variables)]
    pub fn new(nes_state: Arc<Mutex<StateHandler>>, emulator_tx: Sender<EmulatorCommand>) -> Self {
        Self {
            #[cfg(feature = "netplay")]
            netplay_gui: crate::netplay::gui::NetplayGui::new(),
            #[cfg(feature = "debug")]
            debug_gui: DebugGui {
                nes_state: nes_state.clone(),
                emulator_tx,
                speed: 1.0,
                override_speed: false,
            },

            #[cfg(feature = "netplay")]
            nes_state,
        }
    }
}
#[cfg(feature = "debug")]
impl DebugGui {
    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.end_row();

        ui.label(format!(
            "Frame: {}",
            super::NesStateHandler::frame(std::ops::Deref::deref(&self.nes_state.lock().unwrap()))
        ));
        ui.end_row();

        if ui
            .checkbox(&mut self.override_speed, "Override emulation speed")
            .changed()
            && !self.override_speed
        {
            let _ = self.emulator_tx.send(EmulatorCommand::SetSpeed(1.0));
        }

        if self.override_speed {
            ui.end_row();
            ui.add(egui::Slider::new(&mut self.speed, 0.005..=2.0).suffix("x"));
            let _ = self.emulator_tx.send(EmulatorCommand::SetSpeed(self.speed));
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
        self.netplay_gui.ui(ui, &mut self.nes_state.lock().unwrap());
    }

    #[cfg(feature = "netplay")]
    fn messages(
        &self,
        main_menu_state: &crate::main_view::gui::MainMenuState,
    ) -> Option<Vec<String>> {
        self.netplay_gui
            .messages(&self.nes_state.lock().unwrap(), main_menu_state)
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

    #[cfg(all(feature = "netplay", feature = "debug"))]
    fn prepare(&mut self) {
        self.netplay_gui.prepare(&self.nes_state.lock().unwrap());
    }
}
