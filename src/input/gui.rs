use crate::{
    gui::GuiComponent,
    input::{InputId, Inputs, JoypadButton, JoypadInput},
    settings::input::InputConfigurationRef,
    GameRunner,
};
use egui::{Button, Color32, Context, Grid, Label, RichText, Ui, Window};
use std::{collections::HashMap, fmt::Debug, rc::Rc};

#[derive(Debug)]
struct MapRequest {
    input_configuration: InputConfigurationRef,
    button: JoypadButton,
}

pub struct InputSettingsGui {
    mapping_request: Option<MapRequest>,
}

impl InputSettingsGui {
    pub fn new() -> Self {
        Self {
            mapping_request: None,
        }
    }

    fn key_map_ui(
        map_request: &mut Option<MapRequest>,
        ui: &mut Ui,
        available_configurations: &HashMap<InputId, InputConfigurationRef>,
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

impl GuiComponent for InputSettingsGui {
    fn handle_event(&mut self, _event: &winit::event::WindowEvent, _game_runner: &mut GameRunner) {}

    fn ui(
        &mut self,
        ctx: &Context,
        game_runner: &mut GameRunner,
        ui_visible: bool,
        is_open: &mut bool,
    ) {
        if !ui_visible {
            return;
        }
        Window::new(self.name())
            .open(is_open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        InputSettingsGui::key_map_ui(
                            &mut self.mapping_request,
                            ui,
                            &game_runner.settings.input.configurations,
                            &game_runner.inputs,
                            &mut game_runner.settings.input.selected[0],
                            0,
                        );
                    });
                    ui.vertical(|ui| {
                        InputSettingsGui::key_map_ui(
                            &mut self.mapping_request,
                            ui,
                            &game_runner.settings.input.configurations,
                            &game_runner.inputs,
                            &mut game_runner.settings.input.selected[1],
                            1,
                        );
                    });
                });
            });

        if let Some(map_request) = &self.mapping_request {
            if game_runner
                .inputs
                .remap_configuration(&map_request.input_configuration, &map_request.button)
            {
                self.mapping_request = None;
            }
        }
    }

    fn name(&self) -> String {
        "Input".to_string()
    }
}
