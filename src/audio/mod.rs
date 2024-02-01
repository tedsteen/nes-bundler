use std::ops::{Add, RangeInclusive};
use std::time::{Duration, Instant};

use anyhow::Result;
use sdl2::audio::{AudioQueue, AudioSpec, AudioSpecDesired};
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
    pub latency: u8,
    pub volume: u8,
    pub output_device: Option<String>,
}

pub struct Stream {
    output_device_name: Option<String>,
    stretch: Stretch<1>,
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
            audio_queue: Stream::new_audio_queue(
                audio_subsystem,
                &audio_settings.output_device,
                audio_settings.latency,
            )?,
            volume: audio_settings.volume as f32 / 100.0,
        })
    }
    
    fn new_audio_queue(audio_subsystem: &AudioSubsystem, output_device: &Option<String>, latency: u8) -> Result<AudioQueue<i16>> {
        let channels = 1;
        let sample_rate = 44100;
        log::debug!("Starting audio output device '{:?}' latency={}, channels={}, sample_rate={}", output_device, latency, channels, sample_rate);
        let desired_spec = AudioSpecDesired {
            freq: Some(sample_rate),
            channels: Some(channels),
            samples: if latency == 0 {
                None
            } else {
                Some(Stream::latency_to_frames(
                    latency,
                    channels,
                    sample_rate as u32,
                ))
            },
        };
        let output_device = audio_subsystem
            .open_queue::<i16, _>(output_device.as_deref(), &desired_spec)
            .or_else(|_| audio_subsystem.open_queue::<i16, _>(None, &desired_spec))
            .map_err(anyhow::Error::msg)?;
        output_device.resume();
        Ok(output_device)
    }

    fn latency_to_frames(latency: u8, channels: u8, sample_rate: u32) -> u16 {
        let latency_frames = (latency as f64 / 1_000.0) * sample_rate as f64;
        (latency_frames * channels as f64) as u16
    }
    fn frames_to_latency(audio_spec: &AudioSpec) -> u8 {
        ((audio_spec.samples as u64 * 1_000)
            / (audio_spec.channels as u64 * audio_spec.freq as u64)) as u8
    }

    pub fn set_latency(&mut self, latency: u8) {
        if let Ok(audio_queue) = Stream::new_audio_queue(
            self.audio_queue.subsystem(),
            &self.output_device_name,
            latency,
        ) {
            self.audio_queue = audio_queue;
        } else {
            log::error!("Failed to set audio latency to {}", latency);
        }
    }

    pub fn get_supported_latency(&self) -> Option<RangeInclusive<u8>> {
        Some(1..=50)
    }

    pub(crate) fn push_samples(&mut self, new_samples: &[SampleFormat], fps_hint: Fps) {        
        let new_len = ((FPS / fps_hint) * new_samples.len() as f32) as usize;
        let queue_size = self.audio_queue.size();

        let len = if queue_size == 0 {
            log::trace!("underrun");
            new_len + (self.audio_queue.spec().size*2) as usize
            
        } else if queue_size > (self.audio_queue.spec().size * 5) {
            log::trace!("overrun: {}", queue_size);
            new_len/2
        } else {
            new_len
        };

        let stretched = self.stretch.process(&[&new_samples], len)[0];
        //Set volume
        let samples: Vec<i16> = stretched.iter().map(|s| (*s as f32 * self.volume) as SampleFormat).collect();

        self.audio_queue.queue_audio(&samples).unwrap();
    }

    pub(crate) fn set_output_device(&mut self, output_device_name: Option<String>) {
        if self.output_device_name != output_device_name {
            match Stream::new_audio_queue(
                self.audio_queue.subsystem(),
                &output_device_name,
                Stream::frames_to_latency(self.audio_queue.spec()),
            ) {
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
