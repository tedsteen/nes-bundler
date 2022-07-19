use egui::{Context, Slider, Window};

use crate::{ GameRunner };

use super::GuiComponent;

pub(crate) struct AudioSettingsGui { }
impl AudioSettingsGui {
    pub(crate) fn new() -> Self {
        Self {}
    }
}
impl GuiComponent for AudioSettingsGui {
    fn handle_event(
        &mut self,
        _event: &winit::event::WindowEvent,
        _game_runner: &mut GameRunner,
    ) {
    }

    fn ui(&mut self, ctx: &Context, game_runner: &mut GameRunner) {
        Window::new("Audio").collapsible(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Audio latency");
                ui.add(Slider::new(&mut game_runner.settings.audio.latency, 1..=500).suffix("ms"));
            });
        });
    }
}
