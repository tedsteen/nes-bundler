use std::{fmt::{Debug}, rc::Rc, collections::HashMap};

use egui::{Button, Color32, Context, Grid, Label, Slider, Ui, Window, RichText};

use crate::{
    input::{JoypadButton, JoypadInput, InputId}, GameRunner, settings::{InputConfigurationRef}
};

use super::GuiComponent;

#[derive(Debug)]
struct MapRequest {
    input_configuration: InputConfigurationRef,
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

    fn key_map_ui(&mut self, ui: &mut Ui, available_configurations: &HashMap<InputId, InputConfigurationRef>, joypad_input: &JoypadInput, selected_configuration: &mut InputConfigurationRef, pad: usize) {
        ui.label(format!("Joypad #{}", pad));
        egui::ComboBox::from_id_source(format!("joypad-{}", pad))
        .width(160.0)
        .selected_text(format!("{:?}", selected_configuration.borrow().name))
        .show_ui(ui, |ui| {
            let mut sorted_configurations: Vec<&InputConfigurationRef> = available_configurations
            .values()
            .filter(|e| !e.borrow().disconnected)
            .collect();
            
            sorted_configurations.sort_by(|a, b| a.borrow().id.cmp(&b.borrow().id));
            
            for input_configuration in sorted_configurations {
                ui.selectable_value(selected_configuration, Rc::clone(input_configuration), input_configuration.borrow().name.clone());
            }
        });

        let input_configuration = selected_configuration;
        Grid::new(format!("joypadmap_grid_{}", pad))
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        use JoypadButton::*;
                        self.button_map_ui(ui, input_configuration, joypad_input, Up);
                        self.button_map_ui(ui, input_configuration, joypad_input, Down);
                        self.button_map_ui(ui, input_configuration, joypad_input, Left);
                        self.button_map_ui(ui, input_configuration, joypad_input, Right);
                        self.button_map_ui(ui, input_configuration, joypad_input, Start);
                        self.button_map_ui(ui, input_configuration, joypad_input, Select);
                        self.button_map_ui(ui, input_configuration, joypad_input, B);
                        self.button_map_ui(ui, input_configuration, joypad_input, A);
                    });
    }

    fn button_map_ui(
        &mut self,
        ui: &mut Ui,
        input_configuration: &InputConfigurationRef,
        joypad_input: &JoypadInput,
        button: JoypadButton,
    ) {
        let mut text = RichText::new(format!("{:?}", button));
        if joypad_input.is_pressed(button) {
            text = text.color(Color32::from_rgb(255, 255, 255));
        }
        match &self.mapping_request {
            Some(MapRequest { input_configuration: map_conf, button: b }) if map_conf == input_configuration && *b == button => {
                if ui
                    .add(Button::new(RichText::new("Cancel").color(Color32::from_rgb(255, 0, 0))))
                    .clicked()
                {
                    self.mapping_request = None;
                };
            }
            _ => {
                let key_to_map = match input_configuration.borrow().kind {
                    crate::input::InputConfigurationKind::Keyboard(mapping) => {
                        mapping.lookup(&button).map(|v| format!("{:?}", v))
                    },
                    crate::input::InputConfigurationKind::Gamepad(mapping) => {
                        mapping.lookup(&button).map(|v| format!("{:?}", v))
                    },
                }.unwrap_or_else(|| "-".to_string());
                
                if ui.button(key_to_map).clicked() {
                    self.mapping_request = Some(MapRequest { input_configuration: input_configuration.clone(), button });
                }
            }
        }
        ui.add(Label::new(text));

        ui.end_row();
    }
}

impl GuiComponent for SettingsGui {
    fn handle_event(
        &mut self,
        _event: &winit::event::WindowEvent,
        _game_runner: &mut GameRunner,
    ) {
    }

    fn ui(&mut self, ctx: &Context, game_runner: &mut GameRunner) {
        Window::new("Settings").collapsible(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Audio latency");
                ui.add(Slider::new(&mut game_runner.settings.audio.latency, 1..=500).suffix("ms"));
            });
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    self.key_map_ui(ui, &game_runner.settings.input.configurations, &game_runner.inputs.p1, &mut game_runner.settings.input.selected[0], 1);
                });
                ui.vertical(|ui| {
                    self.key_map_ui(ui, &game_runner.settings.input.configurations, &game_runner.inputs.p2, &mut game_runner.settings.input.selected[1], 2);
                });
            });
        });

        if let Some(map_request) = &self.mapping_request {
            if game_runner.inputs.remap_configuration(&map_request.input_configuration, &map_request.button) {
                self.mapping_request = None;
            }
        }
    }
}
