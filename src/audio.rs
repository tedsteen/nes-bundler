use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Sample, StreamConfig};
use ringbuf::{Consumer, Producer};

use crate::settings::audio::AudioSettings;

pub struct Stream {
    stream_config: StreamConfig,
    stream: cpal::Stream,
    latency: u16,
    volume: f32,
    producer: Producer<f32>,
    consumer: Arc<Mutex<Consumer<f32>>>,
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
        let (producer, consumer, stream) =
            Stream::setup_stream(&mut stream_config, latency, None, &output_device);
        Self {
            stream_config,
            latency,
            volume: audio_settings.volume as f32 / 100.0,
            stream,
            producer,
            consumer,
            output_device,
        }
    }

    fn setup_stream(
        stream_config: &mut StreamConfig,
        latency: u16,
        previous_stream: Option<&mut Arc<Mutex<Consumer<f32>>>>,
        output_device: &cpal::Device,
    ) -> (Producer<f32>, Arc<Mutex<Consumer<f32>>>, cpal::Stream) {
        stream_config.channels = 1;

        let sample_rate = stream_config.sample_rate.0 as f32;
        let channels = stream_config.channels as usize;

        let (mut producer, consumer) =
            ringbuf::RingBuffer::new(Stream::calc_buffer_length(latency, sample_rate, channels))
                .split();

        if let Some(previous_stream) = previous_stream {
            let mut previous_stream = previous_stream.lock().unwrap();
            //Fill buffer with previous buffers sound
            for _ in 0..producer.capacity() {
                producer.push(previous_stream.pop().unwrap_or(0.0)).unwrap();
            }
        } else {
            //Fill buffer with silence
            for _ in 0..producer.capacity() {
                producer.push(0.0).unwrap();
            }
        }
        let mut last_sample = 0.0;
        let consumer = Arc::new(Mutex::<Consumer<f32>>::new(consumer));
        let stream = output_device
            .build_output_stream(
                stream_config,
                {
                    let consumer = consumer.clone();
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        let mut consumer = consumer.lock().unwrap();
                        for sample in data {
                            if let Some(sample) = consumer.pop() {
                                last_sample = sample;
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
        (producer, consumer, stream)
    }

    fn calc_buffer_length(latency: u16, sample_rate: f32, channels: usize) -> usize {
        let latency_frames = (latency as f32 / 1_000.0) * sample_rate;
        latency_frames as usize * channels as usize
    }

    pub fn get_latency(&self) -> u16 {
        self.latency
    }
    pub fn set_latency(&mut self, latency: u16) {
        let (producer, consumer, stream) = Stream::setup_stream(
            &mut self.stream_config,
            latency,
            Some(&mut self.consumer),
            &self.output_device,
        );
        self.producer = producer;
        self.consumer = consumer;
        self.stream = stream;
        self.latency = latency;
    }

    pub fn set_volume(&mut self, volume: u8) {
        self.volume = volume as f32 / 100.0;
    }

    pub fn get_sample_rate(&self) -> u64 {
        self.stream_config.sample_rate.0.into()
    }

    pub fn push_sample(&mut self, sample: i16) {
        let _ = self.producer.push(Sample::to_f32(&sample) * self.volume);
    }

    pub fn drain(&mut self) {
        let mut consumer = self.consumer.lock().unwrap();
        for _ in consumer.by_ref() {
            // Consume everything
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
