use crate::settings::gui::GuiComponent;
use egui::{Context, Slider, Window};

use super::Debug;

#[derive(Hash, PartialEq, Eq, Default)]
pub struct DebugGui {
    is_open: bool,
}

impl DebugGui {
    pub fn new() -> Self {
        Self { is_open: false }
    }
}

impl GuiComponent for Debug {
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
                    egui::Grid::new("debug_grid")
                        .num_columns(2)
                        .spacing([10.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.checkbox(&mut self.settings.override_fps, "Override FPS");
                            if self.settings.override_fps {
                                ui.add(Slider::new(&mut self.settings.fps, 1..=120).suffix("FPS"));
                            }
                            ui.end_row();
                        });
                });
            });
    }

    fn name(&self) -> Option<String> {
        Some("Debug".to_string())
    }

    fn open(&mut self) -> &mut bool {
        &mut self.gui.is_open
    }

    fn event(&mut self, _event: &winit::event::Event<()>) {}
}
