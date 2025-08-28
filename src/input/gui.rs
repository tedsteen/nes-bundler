use crate::{
    emulation::Emulator,
    input::{JoypadButton, JoypadState},
    main_view::gui::{GuiComponent, GuiEvent},
    settings::Settings,
};
use egui::{Color32, Grid, RichText, Ui};
use serde::Deserialize;

use super::{InputConfiguration, Inputs, MapRequest, settings::InputSettings};

#[derive(Deserialize, Debug)]
pub struct InputButtonsVoca {
    pub up: String,
    pub down: String,
    pub left: String,
    pub right: String,

    pub select: String,
    pub start: String,

    pub b: String,
    pub a: String,
}

impl Default for InputButtonsVoca {
    fn default() -> Self {
        Self {
            up: "Up".to_string(),
            down: "Down".to_string(),
            left: "Left".to_string(),
            right: "Right".to_string(),

            select: "Select".to_string(),
            start: "Start".to_string(),

            b: "B".to_string(),
            a: "A".to_string(),
        }
    }
}

pub struct InputsGui {
    pub inputs: Inputs,
    mapping_request: Option<MapRequest>,
}

impl InputsGui {
    pub fn new(inputs: Inputs) -> Self {
        Self {
            mapping_request: None,
            inputs,
        }
    }

    fn key_map_ui(
        ui: &mut Ui,
        joypad_state: JoypadState,
        available_configurations: &[InputConfiguration],
        input_settings: &mut InputSettings,
        player: usize,
        mapping_request: &mut Option<MapRequest>,
    ) {
        ui.label(format!("Player {}", player + 1));
        let selected_text = input_settings
            .get_selected_configuration_mut(player)
            .name
            .to_string();
        egui::ComboBox::from_id_salt(format!("joypad-{}", player))
            .width(160.0)
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                for input_configuration in available_configurations {
                    ui.selectable_value(
                        &mut input_settings.selected[player],
                        input_configuration.id.clone(),
                        input_configuration.name.clone(),
                    );
                }
            });

        let input_configuration = input_settings.get_selected_configuration_mut(player);
        Grid::new(format!("joypadmap_grid_{}", player))
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                use JoypadButton::*;
                [Up, Down, Left, Right, Select, Start, B, A]
                    .iter()
                    .for_each(|&button| {
                        Self::button_map_ui(
                            mapping_request,
                            ui,
                            input_configuration,
                            joypad_state,
                            button,
                        );
                    });
            });
    }

    fn button_map_ui(
        map_request: &mut Option<MapRequest>,
        ui: &mut Ui,
        input_configuration: &mut InputConfiguration,
        joypad_state: JoypadState,
        button: JoypadButton,
    ) {
        let mut text = RichText::new(format!("{button}"));
        if joypad_state.is_pressed(button) {
            text = text.color(Color32::from_rgb(255, 255, 255));
        }
        ui.label(text);
        match map_request {
            Some(MapRequest {
                input_id,
                button: b,
            }) if *input_id == input_configuration.id && *b == button => {
                if ui
                    .button(RichText::new("Cancel").color(Color32::from_rgb(255, 0, 0)))
                    .clicked()
                {
                    *map_request = None;
                };
            }
            _ => {
                let key_to_map = match &mut input_configuration.kind {
                    crate::input::InputConfigurationKind::Keyboard(mapping) => {
                        mapping.lookup(&button).map(|v| format!("{v}"))
                    }
                    crate::input::InputConfigurationKind::Gamepad(mapping) => {
                        mapping.lookup(&button).map(|v| format!("{v}"))
                    }
                }
                .unwrap_or_else(|| "-".to_string());

                if ui.button(key_to_map).clicked() {
                    *map_request = Some(MapRequest {
                        input_id: input_configuration.id.clone(),
                        button,
                    });
                }
            }
        }
        ui.end_row();
    }
}

impl GuiComponent for InputsGui {
    fn handle_event(&mut self, gui_event: &GuiEvent) {
        self.inputs.advance(gui_event);
    }

    fn ui(&mut self, ui: &mut Ui, _emulator: &mut Emulator) {
        let instance = &mut self.inputs;
        let input_settings = &mut Settings::current_mut().input;
        let available_configurations = &mut input_settings
            .configurations
            .values()
            .filter(|e| instance.is_connected(e))
            .cloned()
            .collect::<Vec<InputConfiguration>>();

        available_configurations.sort_by(|a, b| a.id.cmp(&b.id));

        let joypad_0 = instance.get_joypad(0);
        let joypad_1 = instance.get_joypad(1);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                Self::key_map_ui(
                    ui,
                    joypad_0,
                    available_configurations,
                    input_settings,
                    0,
                    &mut self.mapping_request,
                );
            });
            ui.vertical(|ui| {
                Self::key_map_ui(
                    ui,
                    joypad_1,
                    available_configurations,
                    input_settings,
                    1,
                    &mut self.mapping_request,
                );
            });
        });

        self.inputs
            .remap_configuration(&mut self.mapping_request, input_settings);
    }

    fn name(&self) -> Option<&str> {
        Some("Input")
    }
}
