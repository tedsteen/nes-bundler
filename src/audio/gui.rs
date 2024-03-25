use crate::settings::{gui::GuiComponent, Settings};
use egui::{Slider, Ui};

use super::Audio;
pub struct AudioGui {}

impl GuiComponent<Audio> for AudioGui {
    fn ui(&mut self, instance: &mut Audio, ui: &mut Ui) {
        let available_device_names =
            Audio::get_available_output_device_names_for_subsystem(&instance.audio_subsystem);
        ui.horizontal(|ui| {
            egui::Grid::new("netplay_grid")
                .num_columns(2)
                .spacing([10.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    let audio_settings = &mut Settings::current().audio;

                    ui.label("Output");
                    let selected_device = &mut audio_settings.output_device;
                    if selected_device.is_none() {
                        *selected_device = instance.get_default_device_name();
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
                                        instance.stream.set_output_device(Some(name))
                                    }
                                }
                            });
                        ui.end_row();
                    }

                    ui.label("Volume");
                    ui.add(Slider::new(&mut audio_settings.volume, 0..=100).suffix("%"));
                });
        });
    }

    fn name(&self) -> Option<String> {
        Some("Audio".to_string())
    }
}
