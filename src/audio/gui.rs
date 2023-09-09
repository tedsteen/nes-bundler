use std::{
    ops::Add,
    time::{Duration, Instant},
};

use crate::settings::gui::{GuiComponent, GuiEvent};
use egui::{Context, Slider, Window};

use super::Audio;

#[derive(Hash, PartialEq, Eq)]
pub struct AudioSettingsGui {
    available_device_names: Option<Vec<String>>,
    next_device_names_clear: Instant,
    is_open: bool,
}

impl Default for AudioSettingsGui {
    fn default() -> Self {
        Self {
            available_device_names: None,
            next_device_names_clear: Instant::now(),
            is_open: false,
        }
    }
}

impl GuiComponent for Audio {
    fn ui(&mut self, ctx: &Context, ui_visible: bool, name: String) {
        if !ui_visible {
            return;
        }

        Window::new(name)
            .open(&mut self.gui.is_open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    egui::Grid::new("netplay_grid")
                        .num_columns(2)
                        .spacing([10.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            let audio_settings = &mut self.settings.borrow_mut().audio;

                            ui.label("Output");
                            let selected_device = &mut audio_settings.output_device;
                            if let Some(selected_text) = selected_device
                                .clone()
                                .or_else(|| self.stream.get_default_device_name())
                            {
                                egui::ComboBox::from_id_source("audio-output")
                                    .width(160.0)
                                    .selected_text(selected_text)
                                    .show_ui(ui, |ui| {
                                        if self.gui.next_device_names_clear < Instant::now() {
                                            self.gui.next_device_names_clear =
                                                Instant::now().add(Duration::new(1, 0));
                                            self.gui.available_device_names = None;
                                        }
                                        for name in self
                                            .gui
                                            .available_device_names
                                            .get_or_insert_with(|| {
                                                self.stream.get_available_output_device_names()
                                            })
                                            .clone()
                                        {
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

                            if let Some(latency_range) = self.stream.get_supported_latency() {
                                ui.label("Latency");
                                if ui
                                    .add(
                                        Slider::new(&mut audio_settings.latency, latency_range)
                                            .suffix("ms"),
                                    )
                                    .changed()
                                {
                                    self.stream.set_latency(audio_settings.latency);
                                }
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
            });
    }

    fn name(&self) -> Option<String> {
        Some("Audio".to_string())
    }

    fn open(&mut self) -> &mut bool {
        &mut self.gui.is_open
    }

    fn event(&mut self, _event: &GuiEvent) {}
}
