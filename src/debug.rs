use crate::{Fps, FPS};

pub struct DebugSettings {
    pub override_fps: bool,
    pub fps: Fps,
}

impl DebugSettings {
    pub(crate) fn new() -> Self {
        Self { override_fps: false, fps: FPS }
    }
}