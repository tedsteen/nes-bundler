use super::GuiComponent;
use crate::GameRunner;
use egui::{Context, Slider, Window};

pub(crate) struct AudioSettingsGui {
    is_open: bool,
}

impl AudioSettingsGui {
    pub(crate) fn new() -> Self {
        Self { is_open: false }
    }
}

impl GuiComponent for AudioSettingsGui {
    fn handle_event(&mut self, _event: &winit::event::WindowEvent, _game_runner: &mut GameRunner) {}

    fn ui(&mut self, ctx: &Context, game_runner: &mut GameRunner) {
        Window::new("Audio")
            .open(&mut self.is_open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Audio latency");
                    ui.add(
                        Slider::new(&mut game_runner.settings.audio.latency, 1..=500).suffix("ms"),
                    );
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
