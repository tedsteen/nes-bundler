use egui::{Button, Color32, Label, Ui};
use winit::event::ElementState;

use crate::{Settings, input::{JoypadButton, JoypadInput, JoypadKeyboardInput}};

#[derive(Debug)]
struct MapRequest {
    pad: u8,
    button: JoypadButton
}

pub(crate) struct SettingsGui {
    mapping_request: Option<MapRequest>
}

impl SettingsGui {
    pub(crate) fn new() -> Self {
        Self { mapping_request: None }
    }
    pub(crate) fn handle_event(&mut self, event: &winit::event::Event<'_, ()>, settings: &mut Settings) {
        if let winit::event::Event::WindowEvent { event: winit::event::WindowEvent::KeyboardInput { input, .. }, .. } = event {
            if let Some(code) = input.virtual_keycode {
                if let ElementState::Pressed = input.state {
                    if let Some(map_request) = &self.mapping_request {
                        let inputs = &mut settings.inputs[map_request.pad as usize];
                        let current_key_code = inputs.keyboard.mapping.lookup(&map_request.button);
                        *current_key_code = code;
                        self.mapping_request = None;
                    }
                }
            }
        }
    }

    fn key_map_ui(self: &mut Self, ui: &mut Ui, keyboard_input: &mut JoypadKeyboardInput, pad: u8) {
        ui.label(format!("Joypad #{}", pad + 1));
        ui.group(|ui| {
            self.make_button_combo(ui, pad, keyboard_input, JoypadButton::UP);
            self.make_button_combo(ui, pad, keyboard_input, JoypadButton::DOWN);
            self.make_button_combo(ui, pad, keyboard_input, JoypadButton::LEFT);
            self.make_button_combo(ui, pad, keyboard_input, JoypadButton::RIGHT);
            self.make_button_combo(ui, pad, keyboard_input, JoypadButton::START);
            self.make_button_combo(ui, pad, keyboard_input, JoypadButton::SELECT);
            self.make_button_combo(ui, pad, keyboard_input, JoypadButton::B);
            self.make_button_combo(ui, pad, keyboard_input, JoypadButton::A);
        });
    }

    fn make_button_combo(&mut self, ui: &mut egui::Ui, pad: u8, keyboard_input: &mut JoypadKeyboardInput, button: JoypadButton) {
        let mut label = Label::new(format!("Pad {} - {:?}", pad + 1, button));
        
        if keyboard_input.is_pressed(button) {
            label = label.text_color(Color32::from_rgb(255, 255, 255));
        }
        let key_to_map = keyboard_input.mapping.lookup(&button);
        ui.horizontal(|ui| {
            ui.label(label);

            match self.mapping_request {
                Some(MapRequest { pad: p, button: b}) if p == pad && b == button => {
                    if ui.add(Button::new("Cancel").text_color(Color32::from_rgb(255, 0, 0))).clicked() {
                        self.mapping_request = None;
                    };
                },
                _ => {
                    if ui.button(format!("{:?}", key_to_map)).clicked() {
                        self.mapping_request = Some(MapRequest { pad, button });
                    }
                }
            }
        });
    }
    
    pub(crate) fn ui(&mut self, ctx: &egui::CtxRef, settings: &mut Settings) {
        egui::Window::new("Settings").collapsible(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Audio latency");
                ui.add(egui::Slider::new(&mut settings.audio_latency, 1..=500).suffix("ms"));
            });
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    self.key_map_ui(ui, &mut settings.inputs[0].keyboard, 0)
                });

                ui.vertical(|ui| {
                    self.key_map_ui(ui, &mut settings.inputs[1].keyboard, 1)
                });
            });
        });
    }
}
