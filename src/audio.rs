use cpal::Sample;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::RingBuffer;

pub(crate) struct Audio {
    output_device: cpal::Device,
    output_config: cpal::SupportedStreamConfig
}

pub(crate) struct Stream {
    #[allow(dead_code)] // This reference needs to be held on to to keep the stream running
    stream: cpal::Stream,
    pub producer: ringbuf::Producer<i16>,
    pub sample_rate: f32,
    pub buffer_length: usize
}

impl Stream {
    fn new(channels: u16, latency : u16, output_device: &cpal::Device, output_config: &cpal::SupportedStreamConfig) -> Self {
        let mut stream_config = output_config.config();
        stream_config.channels = channels;

        stream_config.buffer_size = match output_config.buffer_size() {
            cpal::SupportedBufferSize::Range {min, max: _} => cpal::BufferSize::Fixed(*min),
            cpal::SupportedBufferSize::Unknown =>  cpal::BufferSize::Default,
        };

        let sample_rate = stream_config.sample_rate.0 as f32;
        let channels = stream_config.channels as usize;
        
        let latency_frames = (latency as f32 / 1_000.0) * sample_rate as f32;
        let buffer_length = latency_frames as usize * channels as usize;

        println!("Stream config: {:?}", stream_config);
        println!("latency_frames: {:?}, latency_samples: {:?}", latency_frames, buffer_length);

        let (producer, stream) = match output_config.sample_format() {
            cpal::SampleFormat::F32 => Audio::create_audio_stream::<f32>(output_device, &stream_config, buffer_length),
            cpal::SampleFormat::I16 => Audio::create_audio_stream::<i16>(output_device, &stream_config, buffer_length),
            cpal::SampleFormat::U16 => Audio::create_audio_stream::<u16>(output_device, &stream_config, buffer_length)
        };

        Self {
            stream,
            producer,
            sample_rate,
            buffer_length
        }
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

    pub(crate) fn start(self: &Self, latency : u16, channels: u16) -> Stream {
        Stream::new(channels, latency, &self.output_device, &self.output_config)
    }

    fn create_audio_stream<T>(output_device: &cpal::Device, stream_config: &cpal::StreamConfig, buffer_length: usize) -> (ringbuf::Producer<i16>, cpal::Stream)
    where
    T: cpal::Sample,
    {
        // The buffer to share samples
        let ring = RingBuffer::<i16>::new(buffer_length * 2);
        let (mut producer, mut consumer) = ring.split();

        // Fill the samples with 0.0 equal to the length of the delay.
        for _ in 0..buffer_length {
            producer.push(0).unwrap();
        }
        
        let err_fn = |err| eprintln!("an error occurred on the output audio stream: {}", err);
        let output_stream: cpal::Stream = output_device.build_output_stream(
            stream_config, 
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                //consumer.pop_slice(data);
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
            err_fn).expect("Could not build sound output stream");
        
        output_stream.play().expect("Could not start playing sound output stream");
        (producer, output_stream)
    }
}