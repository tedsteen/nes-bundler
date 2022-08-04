use super::GuiComponent;
use crate::GameRunner;
use egui::{Context, Slider, Window};

pub struct AudioSettingsGui {
    is_open: bool,
}

impl AudioSettingsGui {
    pub fn new() -> Self {
        Self { is_open: true }
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
                            ui.label("Latency");
                            ui.add(
                                Slider::new(&mut game_runner.settings.audio.latency, 1..=70)
                                    .suffix("ms"),
                            );
                            ui.end_row();
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
