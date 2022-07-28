use std::collections::VecDeque;
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
    producer_history: VecDeque<i32>,
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
        let (producer, stream, producer_history) =
            Stream::setup_stream(&mut stream_config, &output_device, &volume);

        Self {
            stream_config,
            latency,
            volume,
            stream,
            producer,
            output_device,
            producer_history,
        }
    }

    fn setup_stream(
        stream_config: &mut StreamConfig,
        output_device: &cpal::Device,
        volume: &Arc<Mutex<f32>>,
    ) -> (Producer<SampleFormat>, cpal::Stream, VecDeque<i32>) {
        stream_config.channels = 1;

        let (producer, mut consumer) =
            ringbuf::RingBuffer::<SampleFormat>::new(100_000)
                .split();

        let sample_count_16ms = Self::calc_buffer_length(16, stream_config) as usize;
        let (mut producer_2, mut consumer_2) =
        ringbuf::RingBuffer::<f32>::new(sample_count_16ms)
            .split();
        let zeros = vec![0.0; sample_count_16ms];
        producer_2.push_slice(zeros.as_slice());

        let stream = output_device
            .build_output_stream(
                stream_config,
                {
                    let volume = volume.clone();
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        let mut last_sample = 0.0;
                        let volume = *volume.lock().unwrap();
                        for sample in data {
                            if let Some(sample) = consumer.pop() {
                                last_sample = Sample::to_f32(&sample);
                                let _ = producer_2.push(last_sample);
                                consumer_2.pop();
                            } else if let Some(sample) = consumer_2.pop() {
                                //println!("Buffer underrun using {sample} instead");
                                last_sample = sample;
                                let _ = producer_2.push(sample * 0.98);
                            }
                            *sample = last_sample * volume;
                        }
                    }
                },
                |err| eprintln!("an error occurred on the output audio stream: {}", err),
            )
            .expect("Could not build sound output stream");

        let mut producer_history = VecDeque::new();
        for _ in 0..10 {
            producer_history.push_back(0);
        }

        (producer, stream, producer_history)
    }

    fn calc_buffer_length(latency: u8, stream_config: &StreamConfig) -> u32 {
        let latency_frames = ((latency as f32 / 1_000.0) * stream_config.sample_rate.0 as f32) as u32;
        latency_frames * stream_config.channels as u32
    }

    pub fn get_latency(&self) -> u8 {
        self.latency
    }

    pub fn set_latency(&mut self, latency: u8) {
        self.stream_config.buffer_size = BufferSize::Fixed(Self::calc_buffer_length(latency, &self.stream_config));
        let (producer, stream, producer_history) = Stream::setup_stream(
            &mut self.stream_config,
            &self.output_device,
            &self.volume,
        );
        self.producer = producer;
        self.stream = stream;
        self.latency = latency;
        self.producer_history = producer_history;
    }

    pub fn set_volume(&mut self, volume: u8) {
        *self.volume.lock().unwrap() = volume as f32 / 100.0;
    }

    pub fn get_sample_rate(&self) -> u64 {
        self.stream_config.sample_rate.0.into()
    }

    pub fn drain(&mut self) {
        let (producer, stream, producer_history) = Stream::setup_stream(
            &mut self.stream_config,
            &self.output_device,
            &self.volume,
        );
        self.producer = producer;
        self.stream = stream;
        self.producer_history = producer_history;
    }

    pub(crate) fn push_samples(&mut self, samples: &[i16]) {
        let max_buff_size = 2000;
        let curr_buff_size = self.producer.len() as u32;
        self.producer_history.push_front(curr_buff_size as i32);
        self.producer_history.pop_back();

        let producer_history = &mut self.producer_history;
        let avg = producer_history.iter().sum::<i32>() / producer_history.len() as i32;

        if avg > max_buff_size {
            println!("Overrun: {avg}, {max_buff_size}");
            for ele in producer_history.iter_mut() {
                *ele = std::cmp::max(0, avg - samples.len() as i32);
            }
        } else {
            self.producer.push_slice(samples);
        }
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
