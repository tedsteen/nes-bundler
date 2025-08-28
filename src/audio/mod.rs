use std::sync::{Arc, Mutex, RwLock};

use serde::{Deserialize, Serialize};

use crate::{
    audio::sdl3_impl::{SDL3AudioStream, SDL3AudioSystem, SDL3AvailableAudioDevice},
    emulation::{StateHandler, VideoBufferPool},
    input::JoypadState,
    settings::MAX_PLAYERS,
};

pub mod gui;
mod sdl3_impl;

pub type AudioSystem = SDL3AudioSystem;
pub type AudioStream = SDL3AudioStream;
pub type AvailableAudioDevice = SDL3AvailableAudioDevice;

pub struct NesBundlerAudioCallback {
    pub frame_buffer: VideoBufferPool,
    pub audio_buffer: Vec<f32>,
    pub nes_state: Arc<Mutex<StateHandler>>,
    pub inputs: Arc<RwLock<[JoypadState; MAX_PLAYERS]>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct AudioSettings {
    pub volume: u8,
    pub output_device: Option<String>,
}

impl AudioSettings {
    // Get's the currently selected device from settings and falls back to the default in case it is missing or not found on the system.
    pub(crate) fn get_selected_device(
        &mut self,
        audio_system: &AudioSystem,
    ) -> AvailableAudioDevice {
        let chosen = self
            .output_device
            .as_ref()
            .and_then(|want| {
                audio_system
                    .get_available_devices()
                    .into_iter()
                    .find(|a| a.name() == *want)
            })
            .unwrap_or_else(|| {
                if self.output_device.is_some() {
                    log::info!(
                        "Selected audio device missing or not found ({:?}); falling back to default",
                        self.output_device
                    );
                }
                audio_system.get_default_device()
            });

        self.output_device = Some(chosen.name());
        chosen
    }
}
