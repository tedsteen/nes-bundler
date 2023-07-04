use crate::{Fps, FPS};
pub mod gui;

pub struct DebugSettings {
    pub override_fps: bool,
    pub fps: Fps,
}

impl DebugSettings {
    pub(crate) fn new() -> Self {
        Self {
            override_fps: false,
            fps: FPS,
        }
    }
}

pub struct Debug {
    pub settings: DebugSettings,
    pub gui: gui::DebugGui,
}
