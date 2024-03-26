use std::time::{Duration, Instant};

use egui::{Align2, Color32, Context, Order, RichText, Ui, Window};

use crate::{
    audio::{gui::AudioGui, Audio},
    input::{gamepad::GamepadEvent, gui::InputsGui, Inputs, KeyEvent},
    nes_state::emulator::{Emulator, EmulatorGui},
    MINIMUM_INTEGER_SCALING_SIZE,
};

pub trait ToGuiEvent {
    /// Convert the struct to a GuiEvent
    fn to_gui_event(&self) -> Option<GuiEvent>;
}

#[derive(Clone, Debug)]
pub enum GuiEvent {
    Keyboard(KeyEvent),
    Gamepad(GamepadEvent),
}

pub trait GuiComponent<T> {
    // Runs every frame
    fn prepare(&mut self, _instance: &mut T) {}

    // Runs if gui is visible
    fn ui(&mut self, _instance: &mut T, _ui: &mut Ui) {}

    fn messages(&self, _instance: &T) -> Option<Vec<String>> {
        None
    }
    fn name(&self) -> Option<String> {
        None
    }
}

enum GuiWithState<'a> {
    Inputs(&'a mut InputsGui, &'a mut Inputs),
    Audio(&'a mut AudioGui, &'a mut Audio),
    Emulator(&'a mut EmulatorGui, &'a mut Emulator),
}

impl GuiWithState<'_> {
    fn ui(&mut self, ui: &mut Ui) {
        match self {
            GuiWithState::Inputs(gui, instance) => gui.ui(instance, ui),
            GuiWithState::Audio(gui, instance) => gui.ui(instance, ui),
            GuiWithState::Emulator(gui, instance) => gui.ui(instance, ui),
        }
    }

    fn messages(&self) -> Option<Vec<String>> {
        match self {
            GuiWithState::Inputs(gui, instance) => gui.messages(instance),
            GuiWithState::Audio(gui, instance) => gui.messages(instance),
            GuiWithState::Emulator(gui, instance) => gui.messages(instance),
        }
    }

    fn name(&self) -> Option<String> {
        match self {
            GuiWithState::Inputs(gui, _) => gui.name(),
            GuiWithState::Audio(gui, _) => gui.name(),
            GuiWithState::Emulator(gui, _) => gui.name(),
        }
    }
    fn prepare(&mut self) {
        match self {
            GuiWithState::Inputs(gui, instance) => gui.prepare(instance),
            GuiWithState::Audio(gui, instance) => gui.prepare(instance),
            GuiWithState::Emulator(gui, instance) => gui.prepare(instance),
        }
    }
}

pub struct SettingsGui {
    inputs_gui: InputsGui,
    audio_gui: AudioGui,
    emulator_gui: EmulatorGui,

    start_time: Instant,
    visible: bool,
}

impl SettingsGui {
    pub fn new() -> Self {
        Self {
            inputs_gui: InputsGui::new(),
            audio_gui: AudioGui::new(),
            emulator_gui: EmulatorGui::new(),
            start_time: Instant::now(),
            visible: false,
        }
    }

    pub fn ui(
        &mut self,
        ctx: &Context,
        audio: &mut Audio,
        inputs: &mut Inputs,
        emulator: &mut Emulator,
    ) {
        let guis = &mut [
            GuiWithState::Audio(&mut self.audio_gui, audio),
            GuiWithState::Inputs(&mut self.inputs_gui, inputs),
            GuiWithState::Emulator(&mut self.emulator_gui, emulator),
        ];
        egui::Area::new("message_area")
            .fixed_pos([0.0, 0.0])
            .order(Order::Middle)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(50.0);

                    for gui in guis.iter_mut() {
                        gui.prepare();
                        if gui.name().is_some() {
                            if let Some(messages) = gui.messages() {
                                for message in messages {
                                    ui.heading(message);
                                }
                            }
                        }
                    }
                    if self.start_time.elapsed() < Duration::new(5, 0) {
                        ui.heading(
                            RichText::new("Press ESC to see settings")
                                .heading()
                                .strong()
                                .background_color(Color32::from_rgba_premultiplied(
                                    20, 20, 20, 180,
                                )),
                        );
                    }
                });
            });

        Window::new("Settings")
            .open(&mut self.visible)
            .collapsible(false)
            .resizable(false)
            .movable(true)
            .pivot(Align2::CENTER_CENTER)
            .default_pos([
                MINIMUM_INTEGER_SCALING_SIZE.0 as f32 / 2.0,
                MINIMUM_INTEGER_SCALING_SIZE.1 as f32 / 2.0,
            ])
            .show(ctx, |ui| {
                for (idx, gui) in guis.iter_mut().enumerate() {
                    if let Some(name) = gui.name() {
                        if idx != 0 {
                            ui.separator();
                        }

                        ui.vertical_centered(|ui| {
                            ui.heading(name);
                        });

                        gui.ui(ui);
                    }
                }
                #[cfg(feature = "debug")]
                {
                    ui.separator();
                    let mut profile = puffin::are_scopes_on();
                    ui.checkbox(&mut profile, "Toggle profiling");
                    puffin::set_scopes_on(profile);
                }
            });
    }

    pub fn toggle_visibility(&mut self) {
        self.visible = !self.visible;
    }
}
