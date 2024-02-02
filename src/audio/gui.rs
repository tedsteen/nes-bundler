use crate::settings::{
    gui::{GuiComponent, GuiEvent},
    Settings,
};
use egui::{Slider, Ui};

use super::Audio;

impl GuiComponent for Audio {
    fn ui(&mut self, ui: &mut Ui, settings: &mut Settings) {
        let available_device_names = self.get_available_output_device_names();
        ui.horizontal(|ui| {
            egui::Grid::new("netplay_grid")
                .num_columns(2)
                .spacing([10.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    let audio_settings = &mut settings.audio;

                    ui.label("Output");
                    let selected_device = &mut audio_settings.output_device;
                    if selected_device.is_none() {
                        *selected_device = self.stream.get_default_device_name();
                    }
                    if let Some(selected_text) = selected_device.as_deref_mut() {
                        egui::ComboBox::from_id_source("audio-output")
                            .width(160.0)
                            .selected_text(selected_text.to_string())
                            .show_ui(ui, |ui| {
                                for name in available_device_names {
                                    if ui
                                        .selectable_value(
                                            selected_device,
                                            Some(name.clone()),
                                            name.clone(),
                                        )
                                        .changed()
                                    {
                                        self.stream.set_output_device(Some(name))
                                    }
                                }
                            });
                        ui.end_row();
                    }

                    ui.label("Volume");
                    if ui
                        .add(Slider::new(&mut audio_settings.volume, 0..=100).suffix("%"))
                        .changed()
                    {
                        self.stream.volume = audio_settings.volume as f32 / 100.0;
                    }
                });
        });
    }

    fn name(&self) -> Option<String> {
        Some("Audio".to_string())
    }

    fn open(&mut self) -> &mut bool {
        &mut self.gui_is_open
    }

    fn event(&mut self, _event: &GuiEvent, _settings: &mut Settings) {}

    fn messages(&self) -> Vec<String> {
        [].to_vec()
    }
}
