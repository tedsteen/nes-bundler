use crate::{settings::gui::GuiComponent, GameRunner};
use egui::{Context, Slider, Window};

#[derive(Hash, PartialEq, Eq)]
pub struct DebugGui {}

impl DebugGui {
    pub fn new() -> Self {
        Self {}
    }
}

impl GuiComponent for DebugGui {
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
                    egui::Grid::new("debug_grid")
                        .num_columns(2)
                        .spacing([10.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.checkbox(&mut game_runner.debug.override_fps, "Override FPS");
                            if game_runner.debug.override_fps {
                                ui.add(
                                    Slider::new(&mut game_runner.debug.fps, 1..=120).suffix("FPS"),
                                );
                            }
                            ui.end_row();
                        });
                });
            });
    }

    fn name(&self) -> String {
        "Debug".to_string()
    }
}
