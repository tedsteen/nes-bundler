use egui_wgpu_backend::egui::{Button, Color32, CtxRef, Grid, Label, Slider, Ui, Window};
use winit::event::ElementState;

use crate::{
    input::{JoypadButton, JoypadInput, JoypadKeyboardInput},
    Settings,
};

#[derive(Debug)]
struct MapRequest {
    pad: usize,
    button: JoypadButton,
}

pub(crate) struct SettingsGui {
    mapping_request: Option<MapRequest>,
}

impl SettingsGui {
    pub(crate) fn new() -> Self {
        Self {
            mapping_request: None,
        }
    }
    pub(crate) fn handle_event(
        &mut self,
        event: &winit::event::WindowEvent,
        settings: &mut Settings,
    ) {
        if let winit::event::WindowEvent::KeyboardInput { input, .. } = event {
            if let Some(code) = input.virtual_keycode {
                if let ElementState::Pressed = input.state {
                    if let Some(map_request) = &self.mapping_request {
                        let inputs = &mut settings.inputs[map_request.pad as usize];
                        let current_key_code = inputs.keyboard.mapping.lookup(&map_request.button);
                        *current_key_code = Some(code);
                        self.mapping_request = None;
                    }
                }
            }
        }
    }

    fn key_map_ui(
        &mut self,
        ui: &mut Ui,
        keyboard_input: &mut JoypadKeyboardInput,
        pad: usize,
    ) {
        ui.label(format!("Joypad #{}", pad + 1));
        Grid::new("joymap_grid")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                use JoypadButton::*;
                self.make_button_combo(ui, pad, keyboard_input, UP);
                self.make_button_combo(ui, pad, keyboard_input, DOWN);
                self.make_button_combo(ui, pad, keyboard_input, LEFT);
                self.make_button_combo(ui, pad, keyboard_input, RIGHT);
                self.make_button_combo(ui, pad, keyboard_input, START);
                self.make_button_combo(ui, pad, keyboard_input, SELECT);
                self.make_button_combo(ui, pad, keyboard_input, B);
                self.make_button_combo(ui, pad, keyboard_input, A);
            });
    }

    fn make_button_combo(
        &mut self,
        ui: &mut Ui,
        pad: usize,
        keyboard_input: &mut JoypadKeyboardInput,
        button: JoypadButton,
    ) {
        let mut label = Label::new(format!("{:?}", button));
        if keyboard_input.is_pressed(button) {
            label = label.text_color(Color32::from_rgb(255, 255, 255));
        }
        ui.add(label);

        match self.mapping_request {
            Some(MapRequest { pad: p, button: b }) if p == pad && b == button => {
                if ui
                    .add(Button::new("Cancel").text_color(Color32::from_rgb(255, 0, 0)))
                    .clicked()
                {
                    self.mapping_request = None;
                };
            }
            _ => {
                let key_to_map = keyboard_input.mapping.lookup(&button);
                let key_to_map = match key_to_map {
                    Some(k) => format!("{:?}", k),
                    None => "-".to_owned(),
                };

                if ui.button(key_to_map).clicked() {
                    self.mapping_request = Some(MapRequest { pad, button });
                }
            }
        }
        ui.end_row();
    }

    pub(crate) fn ui(&mut self, ctx: &CtxRef, settings: &mut Settings) {
        Window::new("Settings").collapsible(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Audio latency");
                ui.add(Slider::new(&mut settings.audio_latency, 1..=500).suffix("ms"));
            });
            ui.horizontal(|ui| {
                for (pad, joypad_inputs) in &mut settings.inputs.iter_mut().enumerate() {
                    ui.vertical(|ui| {
                        self.key_map_ui(ui, &mut joypad_inputs.keyboard, pad);
                    });
                }
            });
        });
    }
}
