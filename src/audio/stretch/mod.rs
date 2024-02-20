use cxx::UniquePtr;

use super::SampleFormat;

#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("nes-bundler/src/audio/stretch/signalsmith-stretch-wrapper.hpp");
        type SignalsmithStretch;

        unsafe fn process(
            self: Pin<&mut SignalsmithStretch>,
            inputs: *const *const i16,
            input_samples: i32,
            outputs: *mut *mut i16,
            output_samples: i32,
        );

        fn signalsmith_stretch_new(
            channels: i32,
            sample_rate: f32,
        ) -> UniquePtr<SignalsmithStretch>;
    }
}
unsafe impl Send for ffi::SignalsmithStretch {}

pub struct Stretch {
    inner: UniquePtr<ffi::SignalsmithStretch>,
    output_buffer: Vec<SampleFormat>,
}

impl Stretch {
    pub fn new() -> Self {
        Self {
            inner: ffi::signalsmith_stretch_new(1, 44100.0),
            output_buffer: vec![0 as SampleFormat; 4096 * 40],
        }
    }

    pub fn process(&mut self, inputs: &[SampleFormat], output_len: usize) -> &[SampleFormat] {
        let outputs = &mut self.output_buffer[0..output_len];

        unsafe {
            self.inner.pin_mut().process(
                [inputs.as_ptr()].as_ptr(),
                inputs.len() as i32,
                [outputs.as_mut_ptr()].as_mut_ptr(),
                outputs.len() as i32,
            );
        }
        outputs
    }
}
