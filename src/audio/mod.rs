use std::ops::Add;
use std::time::{Duration, Instant};

use anyhow::Result;
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::{AudioSubsystem, Sdl};
use serde::{Deserialize, Serialize};

use crate::settings::Settings;
use crate::{Fps, FPS};

use self::stretch::Stretch;

pub mod gui;
mod stretch;

type SampleFormat = i16;

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct AudioSettings {
    pub volume: u8,
    pub output_device: Option<String>,
}

pub struct Stream {
    output_device_name: Option<String>,
    stretch: Stretch,
    audio_queue: AudioQueue<i16>,
    pub(crate) volume: f32,
}

impl Stream {
    pub fn get_available_output_device_names(&self) -> Vec<String> {
        let subsystem = self.audio_queue.subsystem();
        if let Some(num_devices) = subsystem.num_audio_playback_devices() {
            (0..num_devices)
                .flat_map(|i| subsystem.audio_playback_device_name(i))
                .collect()
        } else {
            vec![]
        }
    }

    pub fn get_default_device_name(&self) -> Option<String> {
        self.get_available_output_device_names().first().cloned()
    }

    pub(crate) fn new(
        audio_subsystem: &AudioSubsystem,
        audio_settings: &AudioSettings,
    ) -> Result<Self> {
        Ok(Self {
            output_device_name: audio_settings.output_device.clone(),
            stretch: Stretch::new(),
            audio_queue: Stream::new_audio_queue(audio_subsystem, &audio_settings.output_device)?,
            volume: audio_settings.volume as f32 / 100.0,
        })
    }

    fn new_audio_queue(
        audio_subsystem: &AudioSubsystem,
        output_device: &Option<String>,
    ) -> Result<AudioQueue<i16>> {
        let channels = 1;
        let sample_rate = 44100;
        let desired_spec = AudioSpecDesired {
            freq: Some(sample_rate),
            channels: Some(channels),
            samples: Some(512), //TODO: perhaps figure this value out during runtime
        };
        let output_device = audio_subsystem
            .open_queue::<i16, _>(output_device.as_deref(), &desired_spec)
            .or_else(|_| audio_subsystem.open_queue::<i16, _>(None, &desired_spec))
            .map_err(anyhow::Error::msg)?;
        log::debug!("Starting audio: {:?}", output_device.spec());

        output_device.resume();
        Ok(output_device)
    }

    pub(crate) fn push_samples(&mut self, new_samples: &[SampleFormat], fps_hint: Fps) {
        let new_len = ((FPS / fps_hint) * new_samples.len() as f32) as usize;
        let queue_size = self.audio_queue.size();

        let len = if queue_size == 0 {
            log::trace!("underrun");
            new_len + (self.audio_queue.spec().size * 2) as usize
        } else if queue_size > (self.audio_queue.spec().size * 5) {
            log::trace!("overrun: {}", queue_size);
            new_len / 2
        } else {
            new_len
        };

        let stretched = self.stretch.process(new_samples, len);
        //Set volume
        let samples: Vec<i16> = stretched
            .iter()
            .map(|s| (*s as f32 * self.volume) as SampleFormat)
            .collect();

        self.audio_queue.queue_audio(&samples).unwrap();
    }

    pub(crate) fn set_output_device(&mut self, output_device_name: Option<String>) {
        if self.output_device_name != output_device_name {
            match Stream::new_audio_queue(self.audio_queue.subsystem(), &output_device_name) {
                Ok(audio_queue) => {
                    self.output_device_name = output_device_name;
                    self.audio_queue = audio_queue;
                }
                Err(e) => {
                    log::error!("Failed to set audio output device: {:?}", e);
                }
            }
        }
    }
}

pub struct Audio {
    pub stream: Stream,
    available_device_names: Vec<String>,
    next_device_names_clear: Instant,

    gui_is_open: bool,
}

impl Audio {
    pub fn new(sdl_context: &Sdl, settings: &Settings) -> Result<Self> {
        let audio_subsystem = sdl_context.audio().map_err(anyhow::Error::msg)?;

        Ok(Audio {
            stream: Stream::new(&audio_subsystem, &settings.audio)?,
            available_device_names: vec![],
            next_device_names_clear: Instant::now(),

            gui_is_open: false,
        })
    }

    fn get_available_output_device_names(&self) -> Vec<String> {
        self.available_device_names.clone()
    }

    pub fn sync_audio_devices(&mut self, audio_settings: &mut AudioSettings) {
        if self.next_device_names_clear < Instant::now() {
            self.next_device_names_clear = Instant::now().add(Duration::new(1, 0));
            self.available_device_names = self.stream.get_available_output_device_names();
        }

        let available_device_names = self.get_available_output_device_names();

        let selected_device = &mut audio_settings.output_device;
        if let Some(name) = selected_device {
            if !available_device_names.contains(name) {
                *selected_device = None;
            }
        }
        if selected_device.is_none() {
            *selected_device = self.stream.get_default_device_name();
        }
    }
}
