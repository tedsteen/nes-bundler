use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{StreamConfig, BufferSize, Sample};
use ringbuf::{Producer};

use crate::settings::audio::AudioSettings;
type SampleFormat = i16;

pub struct Stream {
    stream_config: StreamConfig,
    stream: cpal::Stream,
    latency: u8,
    volume: Arc<Mutex<f32>>,
    producer: Producer<SampleFormat>,
    output_device: cpal::Device,
}

impl Stream {
    fn new<T>(
        audio_settings: &AudioSettings,
        mut stream_config: StreamConfig,
        output_device: cpal::Device,
    ) -> Self
    where
        T: cpal::Sample,
    {
        let latency = audio_settings.latency;
        let volume = Arc::new(Mutex::new(audio_settings.volume as f32 / 100.0));
        //stream_config.buffer_size = BufferSize::Fixed(Self::calc_buffer_length(latency, &stream_config));
        let (producer, stream) =
            Stream::setup_stream(&mut stream_config, &output_device, &volume);
        Self {
            stream_config,
            latency,
            volume,
            stream,
            producer,
            output_device,
        }
    }

    fn setup_stream(
        stream_config: &mut StreamConfig,
        output_device: &cpal::Device,
        volume: &Arc<Mutex<f32>>,
    ) -> (Producer<SampleFormat>, cpal::Stream) {
        stream_config.channels = 1;

        let (producer, mut consumer) =
            ringbuf::RingBuffer::<SampleFormat>::new(100_000)
                .split();

        let mut last_sample = 0.0;
        let stream = output_device
            .build_output_stream(
                stream_config,
                {
                    let volume = volume.clone();
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        let volume = *volume.lock().unwrap();
                        for sample in data {
                            if let Some(sample) = consumer.pop() {
                                last_sample = Sample::to_f32(&sample) * volume;
                            } else {
                                //eprintln!("Buffer underrun");
                            }

                            *sample = last_sample;
                        }
                    }
                },
                |err| eprintln!("an error occurred on the output audio stream: {}", err),
            )
            .expect("Could not build sound output stream");
        (producer, stream)
    }

    fn calc_buffer_length(latency: u8, stream_config: &StreamConfig) -> u32 {
        let latency_frames = (latency as f32 / 1_000.0) * stream_config.sample_rate.0 as f32;
        latency_frames as u32 * stream_config.channels as u32
    }

    pub fn get_latency(&self) -> u8 {
        self.latency
    }

    pub fn set_latency(&mut self, latency: u8) {
        self.stream_config.buffer_size = BufferSize::Fixed(Self::calc_buffer_length(latency, &self.stream_config));
        let (producer, stream) = Stream::setup_stream(
            &mut self.stream_config,
            &self.output_device,
            &self.volume,
        );
        self.producer = producer;
        self.stream = stream;
        self.latency = latency;
    }

    pub fn set_volume(&mut self, volume: u8) {
        *self.volume.lock().unwrap() = volume as f32 / 100.0;
    }

    pub fn get_sample_rate(&self) -> u64 {
        self.stream_config.sample_rate.0.into()
    }

    pub fn drain(&mut self) {
        let (producer, stream) = Stream::setup_stream(
            &mut self.stream_config,
            &self.output_device,
            &self.volume,
        );
        self.producer = producer;
        self.stream = stream;
    }

    pub(crate) fn push_samples(&mut self, samples: &[i16]) {
        self.producer.push_slice(samples);
    }
}

pub struct Audio {
    host: cpal::Host,
}

impl Audio {
    pub fn new() -> Self {
        Self {
            host: cpal::default_host(),
        }
    }

    pub fn start(&self, audio_settings: &AudioSettings) -> Stream {
        let output_device = self
            .host
            .default_output_device()
            .expect("no sound output device available");
        println!("Sound output device: {}", output_device.name().unwrap());

        let mut supported_configs_range = output_device
            .supported_output_configs()
            .expect("error while querying configs");
        let output_config = supported_configs_range
            .next()
            .expect("no supported config?!")
            .with_max_sample_rate();

        let stream_config = output_config.config();
        match output_config.sample_format() {
            cpal::SampleFormat::F32 => {
                Stream::new::<f32>(audio_settings, stream_config, output_device)
            }
            cpal::SampleFormat::I16 => {
                Stream::new::<i16>(audio_settings, stream_config, output_device)
            }
            cpal::SampleFormat::U16 => {
                Stream::new::<u16>(audio_settings, stream_config, output_device)
            }
        }
    }
}
