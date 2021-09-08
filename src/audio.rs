use cpal::Sample;
use cpal::traits::{DeviceTrait, HostTrait};
use rusticnes_ui_common::application::RuntimeState;
use std::sync::Arc;
use std::sync::Mutex;
use ringbuf::RingBuffer;
struct Buffer {
    pub latency: u16,
    length: usize,
    sample_rate: f32,
    channels: usize,
    producer: ringbuf::Producer<i16>,
    consumer: ringbuf::Consumer<i16>,
    runtime: Arc<Mutex<RuntimeState>>
}

impl Buffer {
    fn new(runtime: Arc<Mutex<RuntimeState>>, latency: u16, sample_rate: f32, channels: usize) -> Self {
        let (mut producer, consumer, buffer_size) = Buffer::create_ring(&runtime, latency, sample_rate, channels);
        // Fill the samples with 0.0 equal to the length of the delay.
        for _ in 0..buffer_size * 2 {
            producer.push(0).unwrap();
        }

        let apu = &mut runtime.lock().unwrap().nes.apu;
        apu.set_sample_rate(sample_rate as u64);
        

        Self {
            latency,
            length: buffer_size,
            sample_rate,
            channels,
            producer,
            consumer,
            runtime: runtime.clone()
        }
    }
    fn create_ring(_runtime: &Arc<Mutex<RuntimeState>>, latency: u16, sample_rate: f32, channels: usize) -> (ringbuf::Producer<i16>, ringbuf::Consumer<i16>, usize) {
        let latency_frames = (latency as f32 / 1_000.0) * sample_rate as f32;
        let buffer_size = latency_frames as usize * channels as usize;
        let ring = RingBuffer::<i16>::new(buffer_size * 2);
        let (producer, consumer) = ring.split();
        println!("Audio latency: {}ms", latency);
        
        // TODO: set apu sample rate and buffer size here when rusticnes supports it.
        //let apu = &mut runtime.lock().unwrap().nes.apu;
        //apu.set_buffer_size(buffer_size);

        (producer, consumer, buffer_size)
    }

    fn set_latency(self: &mut Self, latency: u16) {
        let (mut producer, consumer, length) = Buffer::create_ring(&self.runtime, latency, self.sample_rate, self.channels);
        self.consumer.move_to(&mut producer, None);
        self.latency = latency;
        self.producer = producer;
        self.consumer = consumer;
        self.length = length;
    }
}

pub(crate) struct Audio {
    output_device: cpal::Device,
    output_config: cpal::SupportedStreamConfig
}

pub(crate) struct Stream {
    #[allow(dead_code)] // This reference needs to be held on to to keep the stream running
    stream: cpal::Stream,
    buffer: Arc<Mutex<Buffer>>,
}

impl Stream {
    fn new(latency : u16, audio: &Audio, runtime: Arc<Mutex<RuntimeState>>) -> Self {
        let mut stream_config = audio.output_config.config();
        stream_config.channels = 1;
/*
        stream_config.buffer_size = match output_config.buffer_size() {
            cpal::SupportedBufferSize::Range {min, max: _} => cpal::BufferSize::Fixed(*min),
            cpal::SupportedBufferSize::Unknown =>  cpal::BufferSize::Default,
        };
*/
        let (stream, buffer) = Stream::create_stream(&audio.output_device, &audio.output_config, &stream_config, latency, &runtime);
        Self {
            stream: stream,
            buffer,
        }
    }

    pub fn set_latency(self: &Self, latency: u16) {
        let mut buffer = self.buffer.lock().unwrap();
        if buffer.latency != latency {
            buffer.set_latency(std::cmp::max(1, latency) as u16);
        }
    }

    fn create_stream(output_device: &cpal::Device, output_config: &cpal::SupportedStreamConfig, stream_config: &cpal::StreamConfig, latency: u16, runtime: &Arc<Mutex<RuntimeState>>) -> (cpal::Stream, Arc<Mutex<Buffer>>) {
        match output_config.sample_format() {
            cpal::SampleFormat::F32 => Stream::create_audio_stream::<f32>(&output_device, &stream_config, latency, runtime),
            cpal::SampleFormat::I16 => Stream::create_audio_stream::<i16>(&output_device, &stream_config, latency, runtime),
            cpal::SampleFormat::U16 => Stream::create_audio_stream::<u16>(&output_device, &stream_config, latency, runtime)
        }
    }

    fn create_audio_stream<T>(output_device: &cpal::Device, stream_config: &cpal::StreamConfig, latency: u16, runtime: &Arc<Mutex<RuntimeState>>) -> (cpal::Stream, Arc<Mutex<Buffer>>)
    where
    T: cpal::Sample,
    {
        let sample_rate = stream_config.sample_rate.0 as f32;
        let channels = stream_config.channels as usize;
        let buffer = Arc::new(Mutex::new(Buffer::new(runtime.clone(), latency, sample_rate, channels)));

        println!("Stream config: {:?}", stream_config);
        
        let runtime = runtime.clone();
        let buffer_for_output_stream = buffer.clone();
        (output_device.build_output_stream(
            stream_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut lock = runtime.lock().unwrap();
                let mut buffer = buffer_for_output_stream.lock().unwrap();

                let apu = &mut lock.nes.apu;
                if apu.buffer_full {
                    apu.buffer_full = false;                    
                    let audio_buffer = apu.output_buffer.to_owned();
                    let result = buffer.producer.push_slice(audio_buffer.as_slice());
                    if result < audio_buffer.len() {
                        eprintln!("Producing audio faster than it's being consumed! ({:?} left)", audio_buffer.len() - result);
                    }
                }
                std::mem::drop(lock);

                let mut input_fell_behind = false;
                for sample in data {
                    *sample = match buffer.consumer.pop() {
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
        ).expect("Could not build sound output stream"), buffer.clone())
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
        Stream::new(latency, self, runtime)
    }
}