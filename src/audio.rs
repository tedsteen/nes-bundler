use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Sample, StreamConfig};
use ringbuf::RingBuffer;
use rusticnes_core::nes::NesState;
use std::sync::Arc;
use std::sync::Mutex;

pub(crate) struct Audio {
    output_device: cpal::Device,
    output_config: cpal::SupportedStreamConfig,
}

pub(crate) struct Stream {
    #[allow(dead_code)] // This reference needs to be held on to to keep the stream running
    stream: cpal::Stream,
    latency: u16,
    nes: Arc<Mutex<NesState>>,
    sample_rate: f32,
    channels: usize,
}

impl Stream {
    fn new<T>(
        latency: u16,
        mut stream_config: StreamConfig,
        output_device: &cpal::Device,
        nes: Arc<Mutex<NesState>>,
    ) -> Self
    where
        T: cpal::Sample,
    {
        stream_config.channels = 1;

        let sample_rate = stream_config.sample_rate.0 as f32;
        let channels = stream_config.channels as usize;

        let buffer_size = Stream::calc_buffer_length(latency, sample_rate, channels);

        let apu = &mut nes.lock().unwrap().apu;
        apu.set_sample_rate(sample_rate as u64);
        apu.set_buffer_size(buffer_size);

        println!("Stream config: {:?}", stream_config);

        let ring =
            RingBuffer::<i16>::new(2 * Stream::calc_buffer_length(1000, sample_rate, channels)); //make a buffer big enough for maximum latency
        let (mut producer, mut consumer) = ring.split();
        // Add the delay to the buffer
        for _ in 0..buffer_size {
            producer.push(0).unwrap();
        }

        let nes_for_stream = nes.clone();
        let start_time = std::time::SystemTime::now();

        let stream = output_device
            .build_output_stream(
                &stream_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    {
                        let apu = &mut nes_for_stream.lock().unwrap().apu;

                        if apu.buffer_full {
                            apu.buffer_full = false;
                            producer.push_slice(apu.output_buffer.to_owned().as_slice());
                        }
                    }

                    let mut input_fell_behind = false;
                    for sample in data {
                        *sample = match consumer.pop() {
                            Some(s) => Sample::from(&s),
                            None => {
                                input_fell_behind = true;
                                0.0
                            }
                        };
                    }
                    if input_fell_behind {
                        if std::time::SystemTime::now()
                            .duration_since(start_time)
                            .unwrap()
                            .gt(&std::time::Duration::from_secs(1))
                        {
                            //eprintln!("Consuming audio faster than it's being produced! Try increasing latency");
                        }
                    }
                },
                |err| eprintln!("an error occurred on the output audio stream: {}", err),
            )
            .expect("Could not build sound output stream");

        Self {
            stream: stream,
            latency,
            nes: nes.clone(),
            sample_rate,
            channels,
        }
    }

    fn calc_buffer_length(latency: u16, sample_rate: f32, channels: usize) -> usize {
        let latency_frames = (latency as f32 / 1_000.0) * sample_rate;
        latency_frames as usize * channels as usize
    }

    pub fn set_latency(self: &mut Self, mut latency: u16) {
        latency = std::cmp::max(latency, 1);

        if self.latency != latency {
            let buffer_size = Stream::calc_buffer_length(latency, self.sample_rate, self.channels);

            let apu = &mut self.nes.lock().unwrap().apu;
            apu.set_buffer_size(buffer_size);
            self.latency = latency;
        }
    }
}

impl Audio {
    pub fn new() -> Self {
        let host = cpal::default_host();

        let output_device = host
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

        Self {
            output_device,
            output_config,
        }
    }

    pub(crate) fn start(self: &Self, latency: u16, nes: Arc<Mutex<NesState>>) -> Stream {
        let stream_config = self.output_config.config();
        /*
                stream_config.buffer_size = match self.output_config.buffer_size() {
                    cpal::SupportedBufferSize::Range {min, max: _} => cpal::BufferSize::Fixed(*min),
                    cpal::SupportedBufferSize::Unknown =>  cpal::BufferSize::Default,
                };
        */
        match self.output_config.sample_format() {
            cpal::SampleFormat::F32 => {
                Stream::new::<f32>(latency, stream_config, &self.output_device, nes)
            }
            cpal::SampleFormat::I16 => {
                Stream::new::<i16>(latency, stream_config, &self.output_device, nes)
            }
            cpal::SampleFormat::U16 => {
                Stream::new::<u16>(latency, stream_config, &self.output_device, nes)
            }
        }
    }
}
