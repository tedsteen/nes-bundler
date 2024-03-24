use egui::{Slider, Ui};

use crate::{
    settings::{gui::GuiComponent, Settings},
    Fps, FPS,
};

pub struct Debug {
    pub override_fps: bool,
    pub fps: Fps,
}

impl Debug {
    pub(crate) fn new() -> Self {
        Self {
            override_fps: false,
            fps: FPS,
        }
    }
}
pub struct DebugGui {}

impl GuiComponent<Debug> for DebugGui {
    fn ui(&mut self, instance: &mut Debug, ui: &mut Ui, _settings: &mut Settings) {
        ui.horizontal(|ui| {
            egui::Grid::new("debug_grid")
                .num_columns(2)
                .spacing([10.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.checkbox(&mut instance.override_fps, "Override FPS");
                    if instance.override_fps {
                        ui.add(Slider::new(&mut instance.fps, 0.5..=180.0).suffix("FPS"));
                    }
                    ui.end_row();
                });
        });
    }

    fn name(&self) -> Option<String> {
        Some("Debug".to_string())
    }
}
