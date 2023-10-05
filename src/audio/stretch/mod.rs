use cxx::UniquePtr;

#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("nes-bundler/src/audio/stretch/signalsmith-stretch-wrapper.hpp");
        type SignalsmithStretch;

        unsafe fn process(
            self: Pin<&mut SignalsmithStretch>,
            inputs: *mut *mut f32,
            input_samples: i32,
            outputs: *mut *mut f32,
            output_samples: i32,
        );

        fn signalsmith_stretch_new(
            channels: i32,
            sample_rate: f32,
        ) -> UniquePtr<SignalsmithStretch>;
    }
}

type SampleFormat = f32;

pub struct Stretch<const CHANNELS: usize> {
    inner: UniquePtr<ffi::SignalsmithStretch>,
    output_buffer: Vec<SampleFormat>,
}

impl<const CHANNELS: usize> Stretch<CHANNELS> {
    fn to_raw(inputs: &mut [&mut [SampleFormat]], size: usize) -> Vec<*mut SampleFormat> {
        inputs
            .iter_mut()
            .map(|inner| {
                inner[0..size]
                    .iter_mut()
                    .map(|f| *f)
                    .collect::<Vec<f32>>()
                    .as_mut_ptr()
            })
            .collect::<Vec<*mut f32>>()
    }

    pub fn new() -> Self {
        Self {
            inner: ffi::signalsmith_stretch_new(CHANNELS as i32, 44100.0),
            output_buffer: vec![0 as SampleFormat; 4096 * 40],
        }
    }
    const EMPTY_BUFFER: [&'static [SampleFormat]; CHANNELS] = [&[0 as SampleFormat]; CHANNELS];

    //TODO: Why does the input have to be mutable?
    pub fn process(
        &mut self,
        inputs: &mut [&mut [SampleFormat]; CHANNELS],
        output_length: usize,
    ) -> [&[SampleFormat]; CHANNELS] {
        //let inputs = &mut inputs[0];
        let input_length: usize = inputs[0].len() / CHANNELS;
        if output_length < 1 {
            return Self::EMPTY_BUFFER;
        }
        let outputs = &mut self.output_buffer[0..output_length];

        let output_length = outputs.len() / CHANNELS;
        let ffi_outputs = &mut outputs.as_mut_ptr();

        unsafe {
            self.inner.pin_mut().process(
                Self::to_raw(inputs, input_length).as_mut_ptr(),
                input_length as i32,
                ffi_outputs,
                output_length as i32,
            );
        }
        self.output_buffer
            .chunks_exact(output_length)
            .collect::<Vec<&[SampleFormat]>>()[..CHANNELS]
            .try_into()
            .unwrap()
    }
}
