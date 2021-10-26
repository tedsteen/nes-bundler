use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Sample, StreamConfig};
use rusticnes_core::nes::NesState;
use std::sync::atomic::AtomicU16;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;
use std::sync::Mutex;
pub(crate) struct Audio {
    output_device: cpal::Device,
    output_config: cpal::SupportedStreamConfig,
}

pub(crate) struct Stream {
    #[allow(dead_code)] // This reference needs to be held on to to keep the stream running
    stream: cpal::Stream,
    latency: Arc<AtomicU16>,
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
        {
            let apu = &mut nes.lock().unwrap().apu;
            apu.set_sample_rate(sample_rate as u64);
            apu.set_buffer_size(buffer_size);
        }
        let (mut producer, mut consumer) =
            ringbuf::RingBuffer::new(Stream::calc_buffer_length(500, sample_rate, channels) * 2)
                .split(); // 500 is max latency

        println!("Stream config: {:?}", stream_config);

        let mut nes_sample = 0;
        let latency = Arc::new(AtomicU16::new(latency));
        tokio::spawn({
            let latency = latency.clone();
            let nes = nes.clone();
            async move {
                loop {
                    {
                        let apu = &mut nes.lock().unwrap().apu;
                        if apu.buffer_full {
                            for sample in apu.output_buffer.to_owned() {
                                if producer.push(sample).is_err() {
                                    //eprintln!("Sound buffer full");
                                }
                            }
                            apu.buffer_full = false;
                        }
                    }
                    let latency = latency.load(SeqCst);
                    tokio::time::sleep(std::time::Duration::from_micros(
                        ((latency * 1000) / 2) as u64,
                    ))
                    .await;
                }
            }
        });

        let stream = output_device
            .build_output_stream(
                &stream_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    for sample in data {
                        if let Some(sample) = consumer.pop() {
                            nes_sample = sample;
                        } else {
                            //eprintln!("Buffer underrun");
                        }

                        *sample = Sample::from(&nes_sample);
                    }
                },
                |err| eprintln!("an error occurred on the output audio stream: {}", err),
            )
            .expect("Could not build sound output stream");

        Self {
            stream,
            latency,
            nes,
            sample_rate,
            channels,
        }
    }

    fn calc_buffer_length(latency: u16, sample_rate: f32, channels: usize) -> usize {
        let latency_frames = (latency as f32 / 1_000.0) * sample_rate;
        latency_frames as usize * channels as usize
    }

    pub fn set_latency(&self, mut latency: u16) {
        latency = std::cmp::max(latency, 1);

        if self.latency.load(SeqCst) != latency {
            let buffer_size = Stream::calc_buffer_length(latency, self.sample_rate, self.channels);
            let apu = &mut self.nes.lock().unwrap().apu;
            apu.set_buffer_size(buffer_size);
            self.latency.store(latency, SeqCst);
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

    pub(crate) fn start(&self, latency: u16, nes: Arc<Mutex<NesState>>) -> Stream {
        let stream_config = self.output_config.config();
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
