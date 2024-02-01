use std::collections::VecDeque;
use std::ops::{Add, RangeInclusive};
use std::time::{Duration, Instant};

use anyhow::Result;
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpec, AudioSpecDesired};
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
    audio_interface: AudioInterface,
    pub(crate) volume: f32,
}

use std::sync::mpsc::{self, Receiver, Sender};

struct SoundFrame {
    buffer: Vec<i16>,
    fps: Fps,
}

struct Ted {
    spec: AudioSpec,
    rx: Receiver<SoundFrame>,
    stretch2: Stretch<1>,
    buffer: VecDeque<i16>,
}
impl Ted {
    fn new(rx: Receiver<SoundFrame>, spec: AudioSpec) -> Self {
        Self {
            spec,
            rx,
            stretch2: Stretch::new(),
            buffer: VecDeque::new(),
        }
    }
}

struct AudioInterface {
    audio_device: AudioDevice<Ted>,
    tx: Sender<SoundFrame>
}

impl AudioCallback for Ted {
    type Channel = i16;

    fn callback(&mut self, samples_out: &mut [Self::Channel]) {
        let mut new_samples = Vec::new();
        let mut fps = 0.0;
        let mut frames = 0;
        for mut e in self.rx.try_iter() {
            fps += e.fps;
            frames += 1;
            new_samples.append(&mut e.buffer);
        }
        let fps = fps / frames as f32;

        let new_len = ((FPS / fps) * new_samples.len() as f32) as usize;
        let total_len = self.buffer.len() + new_len;
        let drift = samples_out.len() as isize - total_len as isize;

        let len = if drift > 0 {
            log::debug!("underrun: {}", drift);
            new_len + (drift*2) as usize
            
        } else if drift < -(self.spec.size as isize * 3) {
            log::debug!("overrun: {}, {}", drift, self.spec.size);
            new_len/2
        } else {
            //println!("self.buffer.len(): {}", self.buffer.len());
            new_len
        };

        for &s in self.stretch2.process(&[&new_samples], len)[0] {
            self.buffer.push_back(s);
        }
        
        for e in samples_out {
            if let Some(sample) = self.buffer.pop_front() {
                *e = sample;
            }
        }
        
    }
}

impl Stream {
    pub fn get_available_output_device_names(&self) -> Vec<String> {
        let subsystem = self.audio_interface.audio_device.subsystem();
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
        let new_audio_device = Stream::new_audio_interface(
            audio_subsystem,
            &audio_settings.output_device,
            audio_settings.latency,
        )?;
        Ok(Self {
            output_device_name: audio_settings.output_device.clone(),
            audio_interface: new_audio_device,
            volume: audio_settings.volume as f32 / 100.0,
        })
    }

    fn new_audio_interface(
        audio_subsystem: &AudioSubsystem,
        output_device: &Option<String>,
        latency: u8,
    ) -> Result<AudioInterface> {
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
        let (tx, rx) = mpsc::channel();
        let audio_device = audio_subsystem.open_playback(None, &desired_spec, |spec| {
            Ted::new(rx, spec)
        }).unwrap();
        audio_device.resume();
        Ok(AudioInterface { audio_device, tx })
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
        if let Ok(new_interface) = Stream::new_audio_interface(
            self.audio_interface.audio_device.subsystem(),
            &self.output_device_name,
            latency,
        ) {
            self.audio_interface = new_interface;
        } else {
            log::error!("Failed to set audio latency to {}", latency);
        }
    }

    pub fn get_supported_latency(&self) -> Option<RangeInclusive<u8>> {
        Some(1..=50)
    }

    pub(crate) fn push_samples(&mut self, samples: &[SampleFormat], fps_hint: Fps) {
        //Set volume
        let samples = samples
           .iter()
           .map(|s| (*s as f32 * self.volume) as SampleFormat)
           .collect::<Vec<SampleFormat>>();

        self.audio_interface.tx.send(SoundFrame { buffer: samples.to_vec(), fps: fps_hint }).unwrap();
    }

    pub(crate) fn set_output_device(&mut self, output_device_name: Option<String>) {
        if self.output_device_name != output_device_name {
            match Stream::new_audio_interface(
                self.audio_interface.audio_device.subsystem(),
                &output_device_name,
                Stream::frames_to_latency(self.audio_interface.audio_device.spec()),
            ) {
                Ok(new_interface) => {
                    self.output_device_name = output_device_name;
                    self.audio_interface = new_interface;
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
