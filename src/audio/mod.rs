use std::cell::RefCell;
use std::ops::RangeInclusive;
use std::rc::Rc;

use sdl2::audio::{AudioQueue, AudioSpec, AudioSpecDesired};
use sdl2::{AudioSubsystem, Sdl};

use crate::settings::Settings;

use self::gui::AudioSettingsGui;
use self::settings::AudioSettings;

pub mod gui;
pub mod settings;

type SampleFormat = i16;

pub struct Stream {
    output_device_name: Option<String>,
    output_device: AudioQueue<i16>,
    pub(crate) volume: f32,
}

impl Stream {
    pub fn get_available_output_device_names(&self) -> Vec<String> {
        let subsystem = self.output_device.subsystem();
        (0..subsystem.num_audio_playback_devices().unwrap())
            .map(|i| subsystem.audio_playback_device_name(i).unwrap())
            .collect()
    }
    pub fn get_default_device_name(&self) -> Option<String> {
        self.get_available_output_device_names().first().cloned()
    }

    pub(crate) fn new(audio_subsystem: &AudioSubsystem, audio_settings: &AudioSettings) -> Self {
        Self {
            output_device_name: audio_settings.output_device.clone(),
            output_device: Stream::start_output_device(
                audio_subsystem,
                &audio_settings.output_device,
                audio_settings.latency,
            ),
            volume: audio_settings.volume as f32 / 100.0,
        }
    }

    fn start_output_device(
        audio_subsystem: &AudioSubsystem,
        output_device: &Option<String>,
        latency: u8,
    ) -> AudioQueue<i16> {
        let channels = 1;
        let sample_rate = 44100;

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
            .unwrap();
        output_device.resume();
        output_device
    }

    fn latency_to_frames(latency: u8, channels: u8, sample_rate: u32) -> u16 {
        let latency_frames = (latency as f64 / 1_000.0) * sample_rate as f64;
        (latency_frames * channels as f64) as u16
    }
    fn frames_to_latency(audio_spec: &AudioSpec) -> u8 {
        ((audio_spec.samples as u64 * 1_000)
            / (audio_spec.channels as u64 * audio_spec.freq as u64)) as u8
    }

    pub fn get_latency(&self) -> u8 {
        Stream::frames_to_latency(self.output_device.spec())
    }

    pub fn set_latency(&mut self, latency: u8) {
        self.output_device = Stream::start_output_device(
            self.output_device.subsystem(),
            &self.output_device_name,
            latency,
        )
    }

    pub fn get_sample_rate(&self) -> u32 {
        self.output_device.spec().freq as u32
    }

    pub fn get_supported_latency(&self) -> Option<RangeInclusive<u8>> {
        Some(1..=50)
    }

    pub fn drain(&mut self) {
        self.output_device.clear()
    }

    pub(crate) fn push_samples(&mut self, samples: &[SampleFormat]) {
        self.output_device
            .queue_audio(
                &samples
                    .iter()
                    .map(|s| (*s as f32 * self.volume) as i16)
                    .collect::<Vec<i16>>(),
            )
            .unwrap();
    }

    pub(crate) fn set_output_device(&mut self, output_device_name: Option<String>) {
        if self.output_device_name != output_device_name {
            self.output_device_name = output_device_name;
            self.output_device = Stream::start_output_device(
                self.output_device.subsystem(),
                &self.output_device_name,
                Stream::frames_to_latency(self.output_device.spec()),
            );
        }
    }
}

pub struct Audio {
    settings: Rc<RefCell<Settings>>,
    gui: AudioSettingsGui,
    pub stream: Stream,
}

impl Audio {
    pub fn new(sdl_context: &Sdl, settings: Rc<RefCell<Settings>>) -> anyhow::Result<Self> {
        let audio_subsystem = sdl_context.audio().map_err(anyhow::Error::msg)?;

        Ok(Audio {
            stream: Stream::new(&audio_subsystem, &settings.clone().borrow().audio),
            settings,
            gui: Default::default(),
        })
    }
}