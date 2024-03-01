use cxx::UniquePtr;

type SampleFormat = f32;

#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("nes-bundler/src/audio/stretch/signalsmith-stretch-wrapper.hpp");
        type SignalsmithStretch;

        unsafe fn process(
            instance: Pin<&mut SignalsmithStretch>,
            inputs: *const *const f32,
            input_samples: i32,
            outputs: *mut *mut f32,
            output_samples: i32,
        );
        unsafe fn presetCheaper(
            self: Pin<&mut SignalsmithStretch>,
            channels: i32,
            sample_rate: f32,
        );

        fn signalsmith_stretch_new() -> UniquePtr<SignalsmithStretch>;
    }
}

pub struct Stretch {
    inner: UniquePtr<ffi::SignalsmithStretch>,
    output_buffer: Vec<SampleFormat>,
}

impl Stretch {
    pub fn new() -> Self {
        Self {
            inner: ffi::signalsmith_stretch_new(),
            output_buffer: vec![0 as SampleFormat; 4096 * 40],
        }
    }
    pub fn preset_cheaper(&mut self, channels: i32, sample_rate: f32) {
        unsafe {
            self.inner.pin_mut().presetCheaper(channels, sample_rate);
        }
    }
    pub fn process(&mut self, inputs: &[SampleFormat], output_len: usize) -> &[SampleFormat] {
        let outputs = &mut self.output_buffer[0..output_len];

        unsafe {
            ffi::process(
                self.inner.pin_mut(),
                [inputs.as_ptr()].as_ptr(),
                inputs.len() as i32,
                [outputs.as_mut_ptr()].as_mut_ptr(),
                outputs.len() as i32,
            );
        }
        outputs
    }
}
