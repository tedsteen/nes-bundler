use egui::{Context, Slider, Window};

use crate::{
    settings::{
        gui::{GuiComponent, GuiEvent},
        Settings,
    },
    Fps, FPS,
};

pub struct Debug {
    pub override_fps: bool,
    pub fps: Fps,

    gui_is_open: bool,
}

impl Debug {
    pub(crate) fn new() -> Self {
        Self {
            gui_is_open: false,
            override_fps: false,
            fps: FPS,
        }
    }
}

impl GuiComponent for Debug {
    fn ui(&mut self, ctx: &Context, ui_visible: bool, name: String, _settings: &mut Settings) {
        if !ui_visible {
            return;
        }
        Window::new(name)
            .open(&mut self.gui_is_open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    egui::Grid::new("debug_grid")
                        .num_columns(2)
                        .spacing([10.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.checkbox(&mut self.override_fps, "Override FPS");
                            if self.override_fps {
                                ui.add(Slider::new(&mut self.fps, 0.5..=180.0).suffix("FPS"));
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
        &mut self.gui_is_open
    }

    fn event(&mut self, _event: &GuiEvent, _settings: &mut Settings) {}
}
