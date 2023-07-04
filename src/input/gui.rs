use crate::{
    input::{InputId, Inputs, JoypadButton, JoypadInput},
    settings::gui::GuiComponent,
};
use egui::{Button, Color32, Context, Grid, Label, RichText, Ui, Window};
use std::{collections::BTreeMap, fmt::Debug, rc::Rc};

use super::{settings::InputConfigurationRef, Input};

#[derive(Debug)]
struct MapRequest {
    input_configuration: InputConfigurationRef,
    button: JoypadButton,
}

pub struct InputSettingsGui {
    mapping_request: Option<MapRequest>,
    is_open: bool,
}

impl Default for InputSettingsGui {
    fn default() -> Self {
        Self {
            mapping_request: None,
            is_open: true,
        }
    }
}

impl InputSettingsGui {
    fn key_map_ui(
        map_request: &mut Option<MapRequest>,
        ui: &mut Ui,
        available_configurations: &BTreeMap<InputId, InputConfigurationRef>,
        inputs: &Inputs,
        selected_configuration: &mut InputConfigurationRef,
        player: usize,
    ) {
        ui.label(format!("Player {}", player + 1));
        let selected_text = selected_configuration.borrow().name.to_string();
        egui::ComboBox::from_id_source(format!("joypad-{}", player))
            .width(160.0)
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                let mut sorted_configurations: Vec<&InputConfigurationRef> =
                    available_configurations
                        .values()
                        .filter(|e| inputs.is_connected(&e.borrow()))
                        .collect();

                sorted_configurations.sort_by(|a, b| a.borrow().id.cmp(&b.borrow().id));

                for input_configuration in sorted_configurations {
                    ui.selectable_value(
                        selected_configuration,
                        Rc::clone(input_configuration),
                        input_configuration.borrow().name.clone(),
                    );
                }
            });

        let input_configuration = selected_configuration;
        let joypad_input = inputs.get_joypad(player);
        Grid::new(format!("joypadmap_grid_{}", player))
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                use JoypadButton::*;
                InputSettingsGui::button_map_ui(
                    map_request,
                    ui,
                    input_configuration,
                    joypad_input,
                    Up,
                );
                InputSettingsGui::button_map_ui(
                    map_request,
                    ui,
                    input_configuration,
                    joypad_input,
                    Down,
                );
                InputSettingsGui::button_map_ui(
                    map_request,
                    ui,
                    input_configuration,
                    joypad_input,
                    Left,
                );
                InputSettingsGui::button_map_ui(
                    map_request,
                    ui,
                    input_configuration,
                    joypad_input,
                    Right,
                );
                InputSettingsGui::button_map_ui(
                    map_request,
                    ui,
                    input_configuration,
                    joypad_input,
                    Start,
                );
                InputSettingsGui::button_map_ui(
                    map_request,
                    ui,
                    input_configuration,
                    joypad_input,
                    Select,
                );
                InputSettingsGui::button_map_ui(
                    map_request,
                    ui,
                    input_configuration,
                    joypad_input,
                    B,
                );
                InputSettingsGui::button_map_ui(
                    map_request,
                    ui,
                    input_configuration,
                    joypad_input,
                    A,
                );
            });
    }

    fn button_map_ui(
        map_request: &mut Option<MapRequest>,
        ui: &mut Ui,
        input_configuration: &InputConfigurationRef,
        joypad_input: JoypadInput,
        button: JoypadButton,
    ) {
        let mut text = RichText::new(format!("{:?}", button));
        if joypad_input.is_pressed(button) {
            text = text.color(Color32::from_rgb(255, 255, 255));
        }
        ui.add(Label::new(text));
        match map_request {
            Some(MapRequest {
                input_configuration: map_conf,
                button: b,
            }) if map_conf == input_configuration && *b == button => {
                if ui
                    .add(Button::new(
                        RichText::new("Cancel").color(Color32::from_rgb(255, 0, 0)),
                    ))
                    .clicked()
                {
                    *map_request = None;
                };
            }
            _ => {
                let key_to_map = match &mut input_configuration.borrow_mut().kind {
                    crate::input::InputConfigurationKind::Keyboard(mapping) => {
                        mapping.lookup(&button).map(|v| format!("{:?}", v))
                    }
                    crate::input::InputConfigurationKind::Gamepad(mapping) => {
                        mapping.lookup(&button).map(|v| format!("{:?}", v))
                    }
                }
                .unwrap_or_else(|| "-".to_string());

                if ui.button(key_to_map).clicked() {
                    *map_request = Some(MapRequest {
                        input_configuration: input_configuration.clone(),
                        button,
                    });
                }
            }
        }
        ui.end_row();
    }
}

impl GuiComponent for Input {
    fn event(&mut self, event: &winit::event::Event<()>) {
        self.inputs.advance(event, self.settings.clone());
    }

    fn ui(&mut self, ctx: &Context, ui_visible: bool, name: String) {
        if !ui_visible {
            return;
        }
        Window::new(name)
            .open(&mut self.gui.is_open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let input_settings = &mut self.settings.borrow_mut().input;
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        InputSettingsGui::key_map_ui(
                            &mut self.gui.mapping_request,
                            ui,
                            &input_settings.configurations,
                            &self.inputs,
                            &mut input_settings.selected[0],
                            0,
                        );
                    });
                    ui.vertical(|ui| {
                        InputSettingsGui::key_map_ui(
                            &mut self.gui.mapping_request,
                            ui,
                            &input_settings.configurations,
                            &self.inputs,
                            &mut input_settings.selected[1],
                            1,
                        );
                    });
                });
            });

        if let Some(map_request) = &self.gui.mapping_request {
            if self
                .inputs
                .remap_configuration(&map_request.input_configuration, &map_request.button)
            {
                self.gui.mapping_request = None;
            }
        }
    }

    fn name(&self) -> Option<String> {
        Some("Input".to_string())
    }

    fn open(&mut self) -> &mut bool {
        &mut self.gui.is_open
    }
}
