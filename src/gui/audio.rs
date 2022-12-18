use std::{
    ops::Add,
    time::{Duration, Instant},
};

use super::GuiComponent;
use crate::{audio::Audio, GameRunner};
use egui::{Context, Slider, Window};

pub struct AudioSettingsGui {
    is_open: bool,
    available_device_names: Option<Vec<String>>,
    next_device_names_clear: Instant,
}

impl AudioSettingsGui {
    pub fn new() -> Self {
        Self {
            is_open: true,
            available_device_names: None,
            next_device_names_clear: Instant::now(),
        }
    }
}

impl GuiComponent for AudioSettingsGui {
    fn handle_event(&mut self, _event: &winit::event::WindowEvent, _game_runner: &mut GameRunner) {}
    fn ui(&mut self, ctx: &Context, game_runner: &mut GameRunner, ui_visible: bool) {
        if !ui_visible {
            return;
        }
        Window::new(self.name())
            .open(&mut self.is_open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    egui::Grid::new("netplay_grid")
                        .num_columns(2)
                        .spacing([10.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label("Output");
                            let selected_device = &mut game_runner.settings.audio.output_device;
                            if let Some(selected_text) =
                                Audio::get_default_device_name().or_else(|| selected_device.clone())
                            {
                                egui::ComboBox::from_id_source("audio-output")
                                    .width(160.0)
                                    .selected_text(selected_text)
                                    .show_ui(ui, |ui| {
                                        if self.next_device_names_clear < Instant::now() {
                                            self.next_device_names_clear =
                                                Instant::now().add(Duration::new(1, 0));
                                            self.available_device_names = None;
                                        }
                                        for name in self
                                            .available_device_names
                                            .get_or_insert_with(|| {
                                                Audio::get_available_output_device_names()
                                            })
                                            .clone()
                                        {
                                            ui.selectable_value(
                                                selected_device,
                                                Some(name.clone()),
                                                name,
                                            );
                                        }
                                    });
                                ui.end_row();
                            }

                            if let Some(latency_range) =
                                game_runner.sound_stream.get_supported_latency()
                            {
                                ui.label("Latency");
                                ui.add(
                                    Slider::new(
                                        &mut game_runner.settings.audio.latency,
                                        latency_range,
                                    )
                                    .suffix("ms"),
                                );
                                ui.end_row();
                            }

                            ui.label("Volume");
                            ui.add(
                                Slider::new(&mut game_runner.settings.audio.volume, 0..=100)
                                    .suffix("%"),
                            );
                        });
                });
            });
    }
    fn is_open(&mut self) -> &mut bool {
        &mut self.is_open
    }

    fn name(&self) -> String {
        "Audio".to_string()
    }
}
