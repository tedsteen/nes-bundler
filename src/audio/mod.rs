use serde::{Deserialize, Serialize};

use crate::audio::sdl3_impl::{SDL3AudioStream, SDL3AudioSystem, SDL3AvailableAudioDevice};

pub mod pacer;

pub mod gui;
mod sdl3_impl;

pub type AudioSystem = SDL3AudioSystem;
pub type AudioStream = SDL3AudioStream;
pub type AvailableAudioDevice = SDL3AvailableAudioDevice;

pub const MAX_AUDIO_LATENCY_MICROS: u32 = 40_000;
pub const MIN_AUDIO_LATENCY_MICROS: u32 = 8_000;

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct AudioSettings {
    pub volume: u8,
    pub output_device: Option<String>,
    #[serde(
        default = "AudioSettings::default_latency_micros",
        deserialize_with = "AudioSettings::de_latency"
    )]
    pub latency_micros: u32, // In μs
}

impl AudioSettings {
    pub const fn default_latency_micros() -> u32 {
        16_000
    }
    fn de_latency<'de, D>(deserializer: D) -> Result<u32, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut v = u32::deserialize(deserializer)?;
        if !(MIN_AUDIO_LATENCY_MICROS..MAX_AUDIO_LATENCY_MICROS).contains(&v) {
            v = AudioSettings::default_latency_micros();
        }
        Ok(v)
    }

    // Resolves the output device from settings and falls back to the system default.
    pub(crate) fn resolve_output_device(&self, audio_system: &AudioSystem) -> AvailableAudioDevice {
        self
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
            })
    }

    pub(crate) fn sync_output_device_selection(&mut self, device: &AvailableAudioDevice) {
        self.output_device = Some(device.name());
    }
}
