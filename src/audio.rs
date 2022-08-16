use std::collections::VecDeque;
use std::ops::RangeInclusive;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{
    BufferSize, ChannelCount, Sample, SampleRate, StreamConfig, SupportedBufferSize,
    SupportedStreamConfig,
};
use ringbuf::{Consumer, Producer};

use crate::settings::audio::AudioSettings;
type SampleFormat = i16;

pub struct Stream {
    output_config: cpal::SupportedStreamConfig,
    stream: cpal::Stream,
    latency: u8,
    volume: Arc<Mutex<f32>>,
    producer: Producer<SampleFormat>,
    output_device: cpal::Device,
    producer_history: VecDeque<i32>,
}

impl Stream {
    fn new(
        output_config: cpal::SupportedStreamConfig,
        audio_settings: &AudioSettings,
        output_device: cpal::Device,
    ) -> Self {
        let latency = audio_settings.latency;
        let volume = Arc::new(Mutex::new(audio_settings.volume as f32 / 100.0));
        let (producer, stream, producer_history) =
            Stream::setup_stream(&output_config, &output_device, &volume, latency);
        Self {
            output_config,
            latency,
            volume,
            stream,
            producer,
            output_device,
            producer_history,
        }
    }

    fn build_internal_stream<T>(
        output_device: &cpal::Device,
        stream_config: &mut StreamConfig,
        volume: &Arc<Mutex<f32>>,
        mut consumer: Consumer<SampleFormat>,
    ) -> cpal::Stream
    where
        T: cpal::Sample + Send + 'static,
    {
        let sample_count_16ms = Self::latency_to_frames(16, stream_config) as usize;
        let (mut producer_2, mut consumer_2) =
            ringbuf::RingBuffer::<T>::new(sample_count_16ms).split();

        let zeros = vec![T::from::<SampleFormat>(&0); sample_count_16ms];
        producer_2.push_slice(zeros.as_slice());
        let mut last_sample = T::from::<SampleFormat>(&0);
        let mut sample_count = 0;
        let channels = stream_config.channels;
        output_device
            .build_output_stream(
                stream_config,
                {
                    let volume = volume.clone();
                    move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                        let volume = *volume.lock().unwrap();
                        for sample in data {
                            if sample_count % channels == 0 {
                                if let Some(sample) = consumer.pop() {
                                    last_sample = Sample::from(&sample);
                                    let _ = producer_2.push(last_sample);
                                    consumer_2.pop();
                                } else if let Some(sample) = consumer_2.pop() {
                                    //println!("Buffer underrun using {sample} instead");
                                    last_sample = sample;
                                    let _ = producer_2
                                        .push(Sample::from(&(Sample::to_f32(&sample) * 0.98)));
                                }
                            }

                            *sample = Sample::from(&(Sample::to_f32(&last_sample) * volume));
                            sample_count += 1;
                        }
                    }
                },
                |err| eprintln!("an error occurred on the output audio stream: {}", err),
            )
            .expect("Could not build sound output stream")
    }

    fn setup_stream(
        output_config: &cpal::SupportedStreamConfig,
        output_device: &cpal::Device,
        volume: &Arc<Mutex<f32>>,
        latency: u8,
    ) -> (Producer<SampleFormat>, cpal::Stream, VecDeque<i32>) {
        let stream_config = &mut output_config.config();
        if let Some(supported_latency) = Self::_get_supported_latency(output_config) {
            let latency = if latency < *supported_latency.start() {
                *supported_latency.start()
            } else if latency > *supported_latency.end() {
                *supported_latency.end()
            } else {
                latency
            };

            stream_config.buffer_size =
                BufferSize::Fixed(Self::latency_to_frames(latency, stream_config));
        };

        let (producer, consumer) = ringbuf::RingBuffer::<SampleFormat>::new(100_000).split();

        let stream = match output_config.sample_format() {
            cpal::SampleFormat::F32 => {
                Self::build_internal_stream::<f32>(output_device, stream_config, volume, consumer)
            }
            cpal::SampleFormat::I16 => {
                Self::build_internal_stream::<i16>(output_device, stream_config, volume, consumer)
            }
            cpal::SampleFormat::U16 => {
                Self::build_internal_stream::<u16>(output_device, stream_config, volume, consumer)
            }
        };

        let mut producer_history = VecDeque::new();
        for _ in 0..10 {
            producer_history.push_back(0);
        }

        (producer, stream, producer_history)
    }

    fn latency_to_frames(latency: u8, stream_config: &StreamConfig) -> u32 {
        let latency_frames = (latency as f64 / 1_000.0) * stream_config.sample_rate.0 as f64;
        (latency_frames * stream_config.channels as f64) as u32
    }
    fn frames_to_latency(frames: u32, channel_count: ChannelCount, sample_rate: &SampleRate) -> u8 {
        ((frames as u64 * 1_000) / (channel_count as u64 * sample_rate.0 as u64)) as u8
    }

    pub fn get_latency(&self) -> u8 {
        self.latency
    }

    pub fn set_latency(&mut self, latency: u8) {
        let (producer, stream, producer_history) = Stream::setup_stream(
            &self.output_config,
            &self.output_device,
            &self.volume,
            latency,
        );
        self.producer = producer;
        self.stream = stream;
        self.latency = latency;
        self.producer_history = producer_history;
    }

    pub fn set_volume(&mut self, volume: u8) {
        *self.volume.lock().unwrap() = volume as f32 / 100.0;
    }

    pub fn get_sample_rate(&self) -> u32 {
        self.output_config.sample_rate().0
    }

    pub fn get_supported_latency(&self) -> Option<RangeInclusive<u8>> {
        Self::_get_supported_latency(&self.output_config)
    }

    fn _get_supported_latency(output_config: &SupportedStreamConfig) -> Option<RangeInclusive<u8>> {
        let channel_count = output_config.channels();
        let sample_rate = &output_config.sample_rate();
        match output_config.buffer_size() {
            SupportedBufferSize::Range { min, max } => Some(RangeInclusive::new(
                std::cmp::max(1, Self::frames_to_latency(*min, channel_count, sample_rate)),
                Self::frames_to_latency(*max, channel_count, sample_rate),
            )),
            SupportedBufferSize::Unknown => None,
        }
    }

    pub fn drain(&mut self) {
        let (producer, stream, producer_history) = Stream::setup_stream(
            &self.output_config,
            &self.output_device,
            &self.volume,
            self.latency,
        );
        self.producer = producer;
        self.stream = stream;
        self.producer_history = producer_history;
    }

    pub(crate) fn push_samples(&mut self, samples: &[SampleFormat]) {
        let max_buff_size = 2000;
        let curr_buff_size = self.producer.len() as u32;
        self.producer_history.push_front(curr_buff_size as i32);
        self.producer_history.pop_back();

        let producer_history = &mut self.producer_history;
        let avg = producer_history.iter().sum::<i32>() / producer_history.len() as i32;

        if avg > max_buff_size {
            for ele in producer_history.iter_mut() {
                *ele = std::cmp::max(0, avg - samples.len() as i32);
            }
        } else {
            self.producer.push_slice(samples);
        }
    }
}

pub struct Audio {}

impl Audio {
    pub fn new() -> Self {
        Self {}
    }

    pub fn start(&self, audio_settings: &AudioSettings) -> Result<Stream, anyhow::Error> {
        let host = cpal::default_host();

        let output_device = host
            .default_output_device()
            .ok_or_else(|| anyhow::Error::msg("Default output device is not available"))?;
        println!("Output device : {}", output_device.name()?);

        let preferred_sample_rate = SampleRate(44100);
        let mut output_configs_with_preferred_sample_rate = output_device
            .supported_output_configs()
            .expect("No supported audio configurations")
            .filter(|c| {
                c.max_sample_rate() >= preferred_sample_rate
                    && c.min_sample_rate() <= preferred_sample_rate
            });
        let nice_match = output_configs_with_preferred_sample_rate
            .find(|c| *c.buffer_size() != SupportedBufferSize::Unknown);

        // Try to use the best match
        let output_config = nice_match
            // or else one with the preferred sample rate
            .or_else(|| output_configs_with_preferred_sample_rate.next())
            .map(|c| c.with_sample_rate(preferred_sample_rate))
            // If all else fails use the default config
            .unwrap_or_else(|| output_device.default_output_config().unwrap());

        println!("Output config : {:?}", output_config);

        Ok(Stream::new(output_config, audio_settings, output_device))
    }
}
