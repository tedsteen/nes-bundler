use std::ops::Add;

use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired, AudioStatus};
use sdl2::{AudioSubsystem, Sdl};
use serde::{Deserialize, Serialize};

use crate::settings::Settings;

pub mod gui;

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct AudioSettings {
    pub volume: u8,
    #[serde(default = "AudioSettings::default_latency")]
    pub latency: u8,
    pub output_device: Option<String>,
}
impl AudioSettings {
    fn default_latency() -> u8 {
        30
    }
}
struct AudioReceiverCallback(AudioReceiver);

impl AudioCallback for AudioReceiverCallback {
    type Channel = f32;

    fn callback(&mut self, out: &mut [f32]) {
        let consumer = &mut self.0;

        let volume = Settings::current().audio.volume as f32 / 100.0;
        let mut missing_samples = 0;
        for s in out {
            if let Ok(new_sample) = consumer.try_recv() {
                *s = new_sample * volume;
            } else {
                missing_samples += 1;
                *s = 0.0;
            }
        }
        if missing_samples > 0 {
            log::warn!("Buffer underrun: {missing_samples} samples");
        }
    }
}
pub type AudioSender = SyncSender<f32>;
pub type AudioReceiver = Receiver<f32>;

pub struct Stream {
    tx: Option<AudioSender>,
    output_device_name: Option<String>,
    audio_device: Option<AudioDevice<AudioReceiverCallback>>,
}

impl Stream {
    pub(crate) fn new(
        audio_subsystem: &AudioSubsystem,
        latency: Duration,
        desired_sample_rate: u32,
    ) -> Result<Self> {
        log::debug!(
            "Trying to start audio: sample rate={desired_sample_rate}, latency={latency:?}"
        );
        let sample_latency =
            (latency.as_secs_f32() * desired_sample_rate as f32 * 1.0).ceil() as u16;

        let (tx, audio_rx) = sync_channel(sample_latency as usize);
        // Fill with silence
        for _ in 0..sample_latency {
            let _ = tx.send(0.0);
        }

        let output_device = &Settings::current().audio.output_device;
        let audio_device = Stream::new_audio_device(
            desired_sample_rate,
            audio_subsystem,
            output_device,
            audio_rx,
        )?;
        Ok(Self {
            tx: Some(tx),
            output_device_name: output_device.clone(),
            audio_device: Some(audio_device),
        })
    }

    pub fn start(&mut self) -> Result<AudioSender> {
        if let Some(device) = &self.audio_device {
            device.resume();
        }
        self.tx.take().ok_or(anyhow!("Stream already started"))
    }

    fn new_audio_device(
        desired_sample_rate: u32,
        audio_subsystem: &AudioSubsystem,
        output_device: &Option<String>,
        audio_rx: AudioReceiver,
    ) -> Result<AudioDevice<AudioReceiverCallback>> {
        let channels = 1;

        let desired_spec = AudioSpecDesired {
            freq: Some(desired_sample_rate as i32),
            channels: Some(channels),
            //Keep this low for a smoother framerate
            samples: Some(10),
        };

        // Make sure the device exists, otherwise default to first available
        let output_device = output_device
            .clone()
            .filter(|name| {
                Audio::get_available_output_device_names_for_subsystem(audio_subsystem)
                    .contains(name)
            })
            .or_else(|| Audio::get_default_device_name_for_subsystem(audio_subsystem));

        let output_device = audio_subsystem
            .open_playback(output_device.as_deref(), &desired_spec, |_| {
                AudioReceiverCallback(audio_rx)
            })
            .map_err(anyhow::Error::msg)?;
        log::info!("Audio started with {:?}", output_device.spec());
        Ok(output_device)
    }

    pub(crate) fn set_output_device(&mut self, output_device_name: Option<String>) {
        if self.output_device_name != output_device_name {
            if let Some(audio_device) = self.audio_device.take() {
                let subsystem = audio_device.subsystem().clone();
                let old_device_status = audio_device.status();
                let desired_sample_rate = audio_device.spec().freq as u32;
                let old_callback = audio_device.close_and_get_callback();

                match Stream::new_audio_device(
                    desired_sample_rate,
                    &subsystem,
                    &output_device_name,
                    old_callback.0,
                ) {
                    Ok(audio_device) => {
                        if old_device_status == AudioStatus::Playing {
                            audio_device.resume();
                        }
                        self.output_device_name = output_device_name;
                        self.audio_device = Some(audio_device);
                    }
                    Err(e) => {
                        log::error!("Failed to set audio output device: {:?}", e);
                    }
                }
            }
        }
    }
}

pub struct Audio {
    pub stream: Stream,
    available_device_names: Vec<String>,
    next_device_names_clear: Instant,
    audio_subsystem: AudioSubsystem,
}

impl Audio {
    pub fn new(sdl_context: &Sdl, latency: Duration, desired_sample_rate: u32) -> Result<Self> {
        let audio_subsystem = sdl_context.audio().map_err(anyhow::Error::msg)?;

        Ok(Audio {
            stream: Stream::new(&audio_subsystem, latency, desired_sample_rate)?,
            available_device_names: vec![],
            next_device_names_clear: Instant::now(),
            audio_subsystem,
        })
    }

    pub fn get_default_device_name_for_subsystem(subsystem: &AudioSubsystem) -> Option<String> {
        Self::get_available_output_device_names_for_subsystem(subsystem)
            .first()
            .cloned()
    }

    pub fn get_default_device_name(&self) -> Option<String> {
        let subsystem = &self.audio_subsystem;
        Self::get_available_output_device_names_for_subsystem(subsystem)
            .first()
            .cloned()
    }

    pub fn get_available_output_device_names_for_subsystem(
        subsystem: &AudioSubsystem,
    ) -> Vec<String> {
        if let Some(num_devices) = subsystem.num_audio_playback_devices() {
            (0..num_devices)
                .flat_map(|i| subsystem.audio_playback_device_name(i))
                .collect()
        } else {
            vec![]
        }
    }

    pub fn sync_audio_devices(&mut self) {
        let available_device_names =
            Self::get_available_output_device_names_for_subsystem(&self.audio_subsystem);
        if self.next_device_names_clear < Instant::now() {
            self.next_device_names_clear = Instant::now().add(Duration::new(1, 0));
            self.available_device_names = available_device_names.clone();
        }

        let selected_device = &mut Settings::current_mut().audio.output_device;
        if let Some(name) = selected_device {
            if !available_device_names.contains(name) {
                *selected_device = None;
            }
        }
        if selected_device.is_none() {
            *selected_device = self.get_default_device_name();
        }
    }
}
