use crate::{
    input::{Inputs, JoypadButton, JoypadInput},
    settings::{
        gui::{GuiComponent, GuiEvent},
        Settings,
    },
};
use egui::{Color32, Grid, RichText, Ui};

use super::{settings::InputConfigurationRef, MapRequest};

impl Inputs {
    fn key_map_ui(
        ui: &mut Ui,
        joypad_input: JoypadInput,
        available_configurations: &[InputConfigurationRef],
        selected_configuration: &mut InputConfigurationRef,
        player: usize,
        mapping_request: &mut Option<MapRequest>,
    ) {
        ui.label(format!("Player {}", player + 1));
        let selected_text = selected_configuration.borrow().name.to_string();
        egui::ComboBox::from_id_source(format!("joypad-{}", player))
            .width(160.0)
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                for input_configuration in available_configurations {
                    ui.selectable_value(
                        selected_configuration,
                        input_configuration.clone(),
                        input_configuration.borrow().name.clone(),
                    );
                }
            });

        let input_configuration = selected_configuration;
        Grid::new(format!("joypadmap_grid_{}", player))
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                use JoypadButton::*;
                [Up, Down, Left, Right, Start, Select, B, A]
                    .iter()
                    .for_each(|&button| {
                        Self::button_map_ui(
                            mapping_request,
                            ui,
                            input_configuration,
                            joypad_input,
                            button,
                        );
                    });
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
        ui.label(text);
        match map_request {
            Some(MapRequest {
                input_configuration: map_conf,
                button: b,
            }) if map_conf == input_configuration && *b == button => {
                if ui
                    .button(RichText::new("Cancel").color(Color32::from_rgb(255, 0, 0)))
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

impl GuiComponent for Inputs {
    fn event(&mut self, event: &GuiEvent, settings: &mut Settings) {
        self.advance(event, settings);
    }

    fn ui(&mut self, ui: &mut Ui, settings: &mut Settings) {
        let input_settings = &mut settings.input;
        let available_configurations = &mut input_settings
            .configurations
            .values()
            .filter(|e| self.is_connected(&e.borrow()))
            .cloned()
            .collect::<Vec<InputConfigurationRef>>();

        available_configurations.sort_by(|a, b| a.borrow().id.cmp(&b.borrow().id));

        let joypad_0 = self.get_joypad(0);
        let joypad_1 = self.get_joypad(1);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                Self::key_map_ui(
                    ui,
                    joypad_0,
                    available_configurations,
                    &mut input_settings.selected[0],
                    0,
                    &mut self.mapping_request,
                );
            });
            ui.vertical(|ui| {
                Self::key_map_ui(
                    ui,
                    joypad_1,
                    available_configurations,
                    &mut input_settings.selected[1],
                    1,
                    &mut self.mapping_request,
                );
            });
        });

        self.remap_configuration();
    }

    fn name(&self) -> Option<String> {
        Some("Input".to_string())
    }

    fn open(&mut self) -> &mut bool {
        &mut self.gui_is_open
    }
    fn messages(&self) -> Vec<String> {
        [].to_vec()
    }
}
