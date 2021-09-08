use cpal::Sample;
use cpal::traits::{DeviceTrait, HostTrait};
use rusticnes_ui_common::application::RuntimeState;
use std::sync::Arc;
use std::sync::Mutex;
use ringbuf::RingBuffer;

pub(crate) struct Audio {
    output_device: cpal::Device,
    output_config: cpal::SupportedStreamConfig
}

pub(crate) struct Stream {
    #[allow(dead_code)] // This reference needs to be held on to to keep the stream running
    stream: cpal::Stream
}

impl Stream {
    fn new(latency : u16, output_device: &cpal::Device, output_config: &cpal::SupportedStreamConfig, runtime: Arc<Mutex<RuntimeState>>) -> Self {
        let mut stream_config = output_config.config();
        stream_config.channels = 1;
/*
        stream_config.buffer_size = match output_config.buffer_size() {
            cpal::SupportedBufferSize::Range {min, max: _} => cpal::BufferSize::Fixed(*min),
            cpal::SupportedBufferSize::Unknown =>  cpal::BufferSize::Default,
        };
*/

        let stream = match output_config.sample_format() {
            cpal::SampleFormat::F32 => Stream::create_audio_stream::<f32>(output_device, &stream_config, latency, runtime),
            cpal::SampleFormat::I16 => Stream::create_audio_stream::<i16>(output_device, &stream_config, latency, runtime),
            cpal::SampleFormat::U16 => Stream::create_audio_stream::<u16>(output_device, &stream_config, latency, runtime)
        };

        Self {
            stream
        }
    }

    fn create_audio_stream<T>(output_device: &cpal::Device, stream_config: &cpal::StreamConfig, latency: u16, runtime: Arc<Mutex<RuntimeState>>) -> cpal::Stream
    where
    T: cpal::Sample,
    {
        let sample_rate = stream_config.sample_rate.0 as f32;
        let channels = stream_config.channels as usize;

        let latency_frames = (latency as f32 / 1_000.0) * sample_rate as f32;
        let buffer_length = latency_frames as usize * channels as usize;

        println!("Stream config: {:?}", stream_config);
        println!("latency_frames: {:?}, latency_samples: {:?}", latency_frames, buffer_length);

        let apu = &mut runtime.lock().unwrap().nes.apu;
        apu.set_sample_rate(sample_rate as u64);
        apu.set_buffer_size(buffer_length);

        // The buffer to share samples
        let ring = RingBuffer::<i16>::new(buffer_length * 2);
        let (mut producer, mut consumer) = ring.split();

        // Fill the samples with 0.0 equal to the length of the delay.
        for _ in 0..buffer_length {
            producer.push(0).unwrap();
        }
        let runtime = Arc::clone(&runtime);

        output_device.build_output_stream(
            stream_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let apu = &mut runtime.lock().unwrap().nes.apu;
                if apu.buffer_full {
                    apu.buffer_full = false;                    
                    let audio_buffer = apu.output_buffer.to_owned();
                    let result = producer.push_slice(audio_buffer.as_slice());
                    if result < audio_buffer.len() {
                        eprintln!("Producing audio faster than it's being consumed! ({:?} left)", audio_buffer.len() - result);
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
                    eprintln!("Consuming audio faster than it's being produced! Try increasing latency");
                }
            },
            |err| eprintln!("an error occurred on the output audio stream: {}", err)
        ).expect("Could not build sound output stream")
    }
}
impl Audio {
    pub fn new() -> Self {
        let host = cpal::default_host();

        let output_device = host.default_output_device().expect("no sound output device available");
        println!("Sound output device: {}", output_device.name().unwrap());

        let mut supported_configs_range = output_device.supported_output_configs().expect("error while querying configs");
        let output_config = supported_configs_range.next().expect("no supported config?!").with_max_sample_rate();

        Self {
            output_device,
            output_config
        }
    }

    pub(crate) fn start(self: &Self, latency : u16, runtime: Arc<Mutex<RuntimeState>>) -> Stream {
        Stream::new(latency, &self.output_device, &self.output_config, runtime)
    }
}