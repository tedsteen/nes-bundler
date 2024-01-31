use egui::{Slider, Ui};

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
            gui_is_open: true,
            override_fps: false,
            fps: FPS,
        }
    }
}

impl GuiComponent for Debug {
    fn ui(&mut self, ui: &mut Ui, _settings: &mut Settings) {
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
    }

    fn name(&self) -> Option<String> {
        Some("Debug".to_string())
    }

    fn open(&mut self) -> &mut bool {
        &mut self.gui_is_open
    }

    fn event(&mut self, _event: &GuiEvent, _settings: &mut Settings) {}
    fn messages(&self) -> Vec<String> {
        [].to_vec()
    }
}
