use std::time::{Duration, Instant};

use egui::{Align2, Color32, Context, Order, RichText, Ui, Window};

use crate::{
    audio::{gui::AudioGui, Audio},
    input::{gamepad::GamepadEvent, gui::InputsGui, Inputs, KeyEvent},
    nes_state::emulator::{Emulator, EmulatorGui, SharedState},
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
    fn ui(&mut self, instance: &mut T, ui: &mut Ui);

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
    Emulator(&'a mut EmulatorGui, &'a mut SharedState),
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
}

pub struct SettingsGui {
    inputs_gui: InputsGui,
    audio_gui: AudioGui,
    emulator_gui: EmulatorGui,

    start_time: Instant,
    visible: bool,
}

impl SettingsGui {
    pub fn new(emulator_gui: EmulatorGui) -> Self {
        Self {
            inputs_gui: InputsGui::new(),
            audio_gui: AudioGui {},
            emulator_gui,
            start_time: Instant::now(),
            visible: false,
        }
    }

    pub fn ui(&mut self, ctx: &Context, emulator: &mut Emulator) {
        let guis = &mut [
            GuiWithState::Audio(&mut self.audio_gui, &mut emulator.audio),
            GuiWithState::Inputs(&mut self.inputs_gui, &mut emulator.inputs),
            GuiWithState::Emulator(&mut self.emulator_gui, &mut emulator.shared_state),
        ];
        egui::Area::new("message_area")
            .fixed_pos([0.0, 0.0])
            .order(Order::Middle)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(50.0);

                    for gui in guis.iter() {
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
            });
    }

    pub fn toggle_visibility(&mut self) {
        self.visible = !self.visible;
    }
}
