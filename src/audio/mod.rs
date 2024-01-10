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
    output_device: AudioQueue<i16>,
    stretch: Stretch<1>,
    pub(crate) volume: f32,
}

impl Stream {
    pub fn get_available_output_device_names(&self) -> Vec<String> {
        let subsystem = self.output_device.subsystem();
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
            output_device: Stream::start_output_device(
                audio_subsystem,
                &audio_settings.output_device,
                audio_settings.latency,
            )?,
            stretch: Stretch::new(),
            volume: audio_settings.volume as f32 / 100.0,
        })
    }

    fn start_output_device(
        audio_subsystem: &AudioSubsystem,
        output_device: &Option<String>,
        latency: u8,
    ) -> Result<AudioQueue<i16>> {
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
        if let Ok(new_device) = Stream::start_output_device(
            self.output_device.subsystem(),
            &self.output_device_name,
            latency,
        ) {
            self.output_device = new_device;
        } else {
            log::error!("Failed to set audio latency to {}", latency);
        }
    }

    pub fn get_supported_latency(&self) -> Option<RangeInclusive<u8>> {
        Some(1..=50)
    }

    pub(crate) fn push_samples(&mut self, samples: &[SampleFormat], fps_hint: Fps) {
        //Set volume
        let samples = &mut samples
            .iter()
            .map(|s| *s as f32 * self.volume)
            .collect::<Vec<f32>>();

        //let mut stretch_amount = FPS / fps_hint;
//
        //let samples_per_frame = (self.output_device.spec().freq as f32 / fps_hint)
        //    + self.output_device.spec().samples as f32;
//
        ////Why 3.5? I don't know, but it works
        //let sdl_max_buffer_size = samples_per_frame * 3.5;
        //let buffer_len = self.output_device.size() + samples.len() as u32;
        //if buffer_len as f32 > sdl_max_buffer_size {
        //    let stretch_multiplier = (sdl_max_buffer_size / buffer_len as f32) * 1.0;
        //    log::trace!(
        //        "audio buffer too big: {:?} multiplier={:?}",
        //        buffer_len,
        //        stretch_multiplier
        //    );
        //    stretch_amount *= stretch_multiplier.max(0.01);
        //} else if buffer_len as f32 <= samples_per_frame {
        //    let stretch_multiplier = (samples_per_frame / buffer_len as f32) * 1.0;
        //    log::trace!(
        //        "audio buffer too small: {:?} multiplier={:?}",
        //        buffer_len,
        //        stretch_multiplier
        //    );
        //    stretch_amount *= stretch_multiplier.max(0.01);
        //}
        //let input_length = samples.len();
        //let output_length = (input_length as f32 * stretch_amount) as usize;
        //if stretch_amount != 1.0 {
        //    log::trace!(
        //        "stretching: amount={:?}% latency={:?} sdl_buffer_len={:?} input_length={:?} output_length={:?}",
        //        stretch_amount * 100.0,
        //        self.output_device.spec().samples,
        //        buffer_len,
        //        input_length,
        //        output_length
        //    );
        //}
//
        //let output_buffer = self.stretch.process(&mut [samples], output_length);
        let output_buffer = [samples];
        if let Err(e) = self.output_device.queue_audio(
            &output_buffer[0]
                .iter()
                .map(|s| *s as i16)
                .collect::<Vec<i16>>(),
        ) {
            log::warn!("Failed to queue audio: {:?}", e);
        }
    }

    pub(crate) fn set_output_device(&mut self, output_device_name: Option<String>) {
        if self.output_device_name != output_device_name {
            match Stream::start_output_device(
                self.output_device.subsystem(),
                &output_device_name,
                Stream::frames_to_latency(self.output_device.spec()),
            ) {
                Ok(new_device) => {
                    self.output_device_name = output_device_name;
                    self.output_device = new_device;
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
