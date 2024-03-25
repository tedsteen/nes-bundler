use std::ops::Add;

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use ringbuf::{Consumer, HeapRb, Producer};
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired, AudioStatus};
use sdl2::{AudioSubsystem, Sdl};
use serde::{Deserialize, Serialize};

use crate::settings::Settings;

pub mod gui;

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct AudioSettings {
    pub volume: u8,
    pub output_device: Option<String>,
}

pub struct Stream {
    tx: Option<AudioSender>,
    output_device_name: Option<String>,
    audio_device: Option<AudioDevice<AudioReceiverCallback>>,
}
pub type AudioSender = Producer<f32, Arc<HeapRb<f32>>>;
pub type AudioReceiver = Consumer<f32, Arc<HeapRb<f32>>>;
struct AudioReceiverCallback(AudioReceiver);

impl AudioCallback for AudioReceiverCallback {
    type Channel = f32;

    fn callback(&mut self, out: &mut [f32]) {
        let consumer = &mut self.0;
        if consumer.len() < out.len() {
            log::warn!("audio underrun: {} < {}", consumer.len(), out.len());
        }

        log::trace!("playing audio samples: {}", out.len().min(consumer.len()));

        // I don't want to hold the lock during the whole sample copy
        #[allow(clippy::clone_on_copy)]
        let volume = Settings::current().audio.volume.clone() as f32 / 100.0;
        for (sample, value) in out.iter_mut().zip(
            consumer
                .pop_iter()
                .map(|s| s * volume) //Set volume
                .chain(core::iter::repeat(0.0)),
        ) {
            *sample = value;
        }
    }
}

impl Stream {
    pub(crate) fn new(audio_subsystem: &AudioSubsystem) -> Result<Self> {
        //TODO: Figure out a good buffer here..
        let (tx, audio_rx) = HeapRb::<f32>::new(1024 * 8).split();

        let audio_settings = &Settings::current().audio;
        Ok(Self {
            tx: Some(tx),
            output_device_name: audio_settings.output_device.clone(),
            audio_device: Some(Stream::new_audio_device(
                audio_subsystem,
                &audio_settings.output_device,
                audio_rx,
            )?),
        })
    }

    pub fn start(&mut self) -> Result<AudioSender> {
        if let Some(device) = &self.audio_device {
            device.resume();
        }
        self.tx.take().ok_or(anyhow!("Stream already started"))
    }

    fn new_audio_device(
        audio_subsystem: &AudioSubsystem,
        output_device: &Option<String>,
        audio_rx: AudioReceiver,
    ) -> Result<AudioDevice<AudioReceiverCallback>> {
        let channels = 1;
        let sample_rate = 44100;
        let desired_spec = AudioSpecDesired {
            freq: Some(sample_rate),
            channels: Some(channels),
            samples: None,
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
        log::debug!("Starting audio: {:?}", output_device.spec());
        Ok(output_device)
    }

    pub(crate) fn set_output_device(&mut self, output_device_name: Option<String>) {
        if self.output_device_name != output_device_name {
            if let Some(audio_device) = self.audio_device.take() {
                let subsystem = audio_device.subsystem().clone();
                let old_device_status = audio_device.status();
                let old_callback = audio_device.close_and_get_callback();

                match Stream::new_audio_device(&subsystem, &output_device_name, old_callback.0) {
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
    pub fn new(sdl_context: &Sdl) -> Result<Self> {
        let audio_subsystem = sdl_context.audio().map_err(anyhow::Error::msg)?;

        Ok(Audio {
            stream: Stream::new(&audio_subsystem)?,
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

        let selected_device = &mut Settings::current().audio.output_device;
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
