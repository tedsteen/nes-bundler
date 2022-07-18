use std::fmt::{Debug};

use egui::{Button, Color32, Context, Grid, Label, Slider, Ui, Window, RichText};

use crate::{
    input::{JoypadButton, JoypadKeyMap, JoypadInput, InputConfigurationKind, InputConfiguration}, GameRunner, settings::{InputSettings}
};

use super::GuiComponent;

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
    fn map_grid_ui<T>(&mut self, ui: &mut Ui, mapping: &mut JoypadKeyMap<T>, joypad_input: &JoypadInput, pad: usize)
    where
        T: PartialEq + Debug
    {
        Grid::new(format!("joymap_grid_1_{}", pad))
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        use JoypadButton::*;
                        self.make_button_combo(ui, pad, mapping, joypad_input, Up);
                        self.make_button_combo(ui, pad, mapping, joypad_input, Down);
                        self.make_button_combo(ui, pad, mapping, joypad_input, Left);
                        self.make_button_combo(ui, pad, mapping, joypad_input, Right);
                        self.make_button_combo(ui, pad, mapping, joypad_input, Start);
                        self.make_button_combo(ui, pad, mapping, joypad_input, Select);
                        self.make_button_combo(ui, pad, mapping, joypad_input, B);
                        self.make_button_combo(ui, pad, mapping, joypad_input, A);
                    });
    }

    fn key_map_ui(&mut self, ui: &mut Ui, input_settings: &mut InputSettings, joypad_input: &JoypadInput, pad: usize) {
        
        let input_configuration: &InputConfiguration = input_settings.get_config(pad);

        egui::ComboBox::from_label(format!("Joypad #{}", pad + 1))
        .width(160.0)
        .selected_text(format!("{:?}", input_configuration.name))
        .show_ui(ui, |ui| {
            let selected_input = &mut input_settings.selected[pad];
            let mut sorted_configurations: Vec<&InputConfiguration> = input_settings.configurations
            .values()
            .filter(|&e| !e.disconnected)
            .collect();
            
            sorted_configurations.sort_by(|&a, &b| a.id.cmp(&b.id));

            for input_configuration in sorted_configurations {
                ui.selectable_value(selected_input, Some(input_configuration.id.clone()), input_configuration.name.clone());
            }
        });

        let input_configuration = input_settings.get_config(pad);
        match &mut input_configuration.kind {
            InputConfigurationKind::Keyboard(mapping) => {
                self.map_grid_ui(ui, mapping, joypad_input, pad);
            },
            InputConfigurationKind::Gamepad(mapping) => {
                self.map_grid_ui(ui, mapping, joypad_input, pad);
            },
        }
    }

    fn make_button_combo<T>(
        &mut self,
        ui: &mut Ui,
        pad: usize,
        mapping: &mut JoypadKeyMap<T>,
        joypad_input: &JoypadInput,
        button: JoypadButton,
    ) where
        T: PartialEq + Debug
    {
        let mut text = RichText::new(format!("{:?}", button));
        if joypad_input.is_pressed(button) {
            text = text.color(Color32::from_rgb(255, 255, 255));
        }
        match self.mapping_request {
            Some(MapRequest { pad: p, button: b }) if p == pad && b == button => {
                if ui
                    .add(Button::new(RichText::new("Cancel").color(Color32::from_rgb(255, 0, 0))))
                    .clicked()
                {
                    self.mapping_request = None;
                };
            }
            _ => {
                let key_to_map = mapping.lookup(&button);
                let key_to_map = match key_to_map {
                    Some(k) => format!("{:?}", k),
                    None => "-".to_string(),
                };

                if ui.button(key_to_map).clicked() {
                    self.mapping_request = Some(MapRequest { pad, button });
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
                    self.key_map_ui(ui, &mut game_runner.settings.input, &game_runner.inputs.p1, 0);
                });
                ui.vertical(|ui| {
                    self.key_map_ui(ui, &mut game_runner.settings.input, &game_runner.inputs.p2, 1);
                });
            });
        });

        if let Some(map_request) = &self.mapping_request {
            let input_configuration = game_runner.settings.input.get_config(map_request.pad);
            if game_runner.inputs.remap_configuration(input_configuration, &map_request.button) {
                self.mapping_request = None;
            }
        }
    }
}
