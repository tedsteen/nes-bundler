#![allow(dead_code)]
use std::f32::consts::PI;

pub struct Resampler {
    filter_chain: FilterChain,
    clock_rate: u64,
    current_cycle: u64,
    next_sample_at: u64,
    generated_samples: u64,
    target_sample_rate: u64,
}

impl Resampler {
    pub fn new(clock_rate: u64, target_sample_rate: u64) -> Self {
        Self {
            filter_chain: construct_lq_filter_chain(
                clock_rate as f32,
                target_sample_rate as f32,
                FilterType::FamiCom,
            ),
            clock_rate,
            current_cycle: 0,
            next_sample_at: 0,
            generated_samples: 0,
            target_sample_rate,
        }
    }
    pub fn process(&mut self, audio: &[f32]) -> Vec<f32> {
        let mut result = vec![];

        for &sample in audio {
            self.filter_chain
                .consume(sample, 1.0 / self.clock_rate as f32);
            if self.current_cycle >= self.next_sample_at {
                result.push(self.filter_chain.output());
                self.generated_samples += 1;
                self.next_sample_at =
                    ((self.generated_samples + 1) * self.clock_rate) / self.target_sample_rate;
            }

            self.current_cycle += 1;
        }

        result
    }
}

trait DspFilter: Send {
    fn consume(&mut self, sample: f32);
    fn output(&self) -> f32;
}

struct IdentityFilter {
    sample: f32,
}

impl IdentityFilter {
    fn new() -> IdentityFilter {
        IdentityFilter { sample: 0.0 }
    }
}

impl DspFilter for IdentityFilter {
    fn consume(&mut self, new_input: f32) {
        self.sample = new_input;
    }

    fn output(&self) -> f32 {
        self.sample
    }
}

struct HighPassIIR {
    alpha: f32,
    previous_output: f32,
    previous_input: f32,
    delta: f32,
}

impl HighPassIIR {
    fn new(sample_rate: f32, cutoff_frequency: f32) -> HighPassIIR {
        let delta_t = 1.0 / sample_rate;
        let time_constant = 1.0 / cutoff_frequency;
        let alpha = time_constant / (time_constant + delta_t);
        HighPassIIR {
            alpha,
            previous_output: 0.0,
            previous_input: 0.0,
            delta: 0.0,
        }
    }
}

impl DspFilter for HighPassIIR {
    fn consume(&mut self, new_input: f32) {
        self.previous_output = self.output();
        self.delta = new_input - self.previous_input;
        self.previous_input = new_input;
    }

    fn output(&self) -> f32 {
        self.alpha * self.previous_output + self.alpha * self.delta
    }
}

struct LowPassIIR {
    alpha: f32,
    previous_output: f32,
    delta: f32,
}

impl LowPassIIR {
    fn new(sample_rate: f32, cutoff_frequency: f32) -> LowPassIIR {
        let delta_t = 1.0 / sample_rate;
        let time_constant = 1.0 / (2.0 * PI * cutoff_frequency);
        let alpha = delta_t / (time_constant + delta_t);
        LowPassIIR {
            alpha,
            previous_output: 0.0,
            delta: 0.0,
        }
    }
}

impl DspFilter for LowPassIIR {
    fn consume(&mut self, new_input: f32) {
        self.previous_output = self.output();
        self.delta = new_input - self.previous_output;
    }

    fn output(&self) -> f32 {
        self.previous_output + self.alpha * self.delta
    }
}

#[allow(non_snake_case)]
fn blackman_window(index: usize, window_size: usize) -> f32 {
    let i = index as f32;
    let M = window_size as f32;
    0.42 - 0.5 * ((2.0 * PI * i) / M).cos() + 0.08 * ((4.0 * PI * i) / M).cos()
}

#[allow(non_snake_case)]
fn sinc(index: usize, window_size: usize, fc: f32) -> f32 {
    let i = index as f32;
    let M = window_size as f32;
    let shifted_index = i - (M / 2.0);
    if index == (window_size / 2) {
        2.0 * PI * fc
    } else {
        (2.0 * PI * fc * shifted_index).sin() / shifted_index
    }
}

fn normalize(input: Vec<f32>) -> Vec<f32> {
    let sum: f32 = input.clone().into_iter().sum();

    input.into_iter().map(|x| x / sum).collect()
}

fn windowed_sinc_kernel(fc: f32, window_size: usize) -> Vec<f32> {
    let mut kernel: Vec<f32> = Vec::new();
    for i in 0..=window_size {
        kernel.push(sinc(i, window_size, fc) * blackman_window(i, window_size));
    }
    normalize(kernel)
}

struct LowPassFIR {
    kernel: Vec<f32>,
    inputs: Vec<f32>,
    input_index: usize,
}

impl LowPassFIR {
    fn new(sample_rate: f32, cutoff_frequency: f32, window_size: usize) -> LowPassFIR {
        let fc = cutoff_frequency / sample_rate;
        let kernel = windowed_sinc_kernel(fc, window_size);
        let mut inputs = Vec::new();
        inputs.resize(window_size + 1, 0.0);

        LowPassFIR {
            kernel,
            inputs,
            input_index: 0,
        }
    }
}

impl DspFilter for LowPassFIR {
    fn consume(&mut self, new_input: f32) {
        self.inputs[self.input_index] = new_input;
        self.input_index = (self.input_index + 1) % self.inputs.len();
    }

    fn output(&self) -> f32 {
        let mut output: f32 = 0.0;
        for i in 0..self.inputs.len() {
            let buffer_index = (self.input_index + i) % self.inputs.len();
            output += self.kernel[i] * self.inputs[buffer_index];
        }
        output
    }
}

// essentially a thin wrapper around a DspFilter, with some bonus data to track
// state when used in a larger chain
struct ChainedFilter {
    wrapped_filter: Box<dyn DspFilter>,
    sampling_period: f32,
    period_counter: f32,
}

struct FilterChain {
    filters: Vec<ChainedFilter>,
}

impl FilterChain {
    fn new() -> FilterChain {
        let identity = IdentityFilter::new();
        FilterChain {
            filters: vec![ChainedFilter {
                wrapped_filter: Box::new(identity),
                sampling_period: 1.0,
                period_counter: 0.0,
            }],
        }
    }

    fn add(&mut self, filter: Box<dyn DspFilter>, sample_rate: f32) {
        self.filters.push(ChainedFilter {
            wrapped_filter: filter,
            sampling_period: (1.0 / sample_rate),
            period_counter: 0.0,
        });
    }

    fn consume(&mut self, input_sample: f32, delta_time: f32) {
        // Always advance the identity filter with the new current sample
        self.filters[0].wrapped_filter.consume(input_sample);
        // Now for every remaining filter in the chain, advance and sample the previous
        // filter as required
        for i in 1..self.filters.len() {
            let previous = i - 1;
            let current = i;
            self.filters[current].period_counter += delta_time;
            while self.filters[current].period_counter >= self.filters[current].sampling_period {
                self.filters[current].period_counter -= self.filters[current].sampling_period;
                let previous_output = self.filters[previous].wrapped_filter.output();
                self.filters[current]
                    .wrapped_filter
                    .consume(previous_output);
            }
        }
    }

    fn output(&self) -> f32 {
        let final_filter = self.filters.last().unwrap();
        final_filter.wrapped_filter.output()
    }
}

enum FilterType {
    Nes,
    FamiCom,
}

fn construct_hq_filter_chain(
    clock_rate: f32,
    target_sample_rate: f32,
    filter_type: FilterType,
) -> FilterChain {
    // https://wiki.nesdev.org/w/index.php?title=APU_Mixer

    // First, no matter what the hardware specifies, we'll do a lightweight downsample to around 8x
    // the target sample rate. This is to somewhat reduce the CPU cost of the rest of the chain
    let mut chain = FilterChain::new();
    let intermediate_samplerate = target_sample_rate * (2.0 + (std::f32::consts::PI / 32.0));
    let intermediate_cutoff_frequency = target_sample_rate * 0.4;
    // This IIR isn't especially sharp, but that's okay. We'll do a better filter later
    // to deal with any aliasing this leaves behind
    chain.add(
        Box::new(LowPassIIR::new(clock_rate, intermediate_cutoff_frequency)),
        clock_rate,
    );

    match filter_type {
        FilterType::Nes => {
            //The NES hardware follows the DACs with a surprisingly involved circuit that adds several low-pass and high-pass filters:

            // A first-order high-pass filter at 90 Hz
            chain.add(
                Box::new(HighPassIIR::new(intermediate_samplerate, 90.0)),
                intermediate_samplerate,
            );
            //  Another first-order high-pass filter at 440 Hz
            chain.add(
                Box::new(HighPassIIR::new(intermediate_samplerate, 440.0)),
                intermediate_samplerate,
            );
            // A first-order low-pass filter at 14 kHz
            chain.add(
                Box::new(LowPassIIR::new(intermediate_samplerate, 14000.0)),
                intermediate_samplerate,
            );
        }
        FilterType::FamiCom => {
            // The Famicom hardware instead ONLY specifies a first-order high-pass filter at 37 Hz,
            // followed by the unknown (and varying) properties of the RF modulator and demodulator.
            chain.add(
                Box::new(HighPassIIR::new(intermediate_samplerate, 37.0)),
                intermediate_samplerate,
            );
        }
    }

    // Finally, perform a high-quality low pass, the result of which will be decimated to become the final output
    // TODO: 160 is huge! That was needed when going from 1.7 MHz -> 44.1 kHz; is it still needed when the source
    // is more like 88.2 kHz? Figure out if we can lower this, it's very expensive.
    let window_size = 160;
    let cutoff_frequency = target_sample_rate * 0.45;
    chain.add(
        Box::new(LowPassFIR::new(
            intermediate_samplerate,
            cutoff_frequency,
            window_size,
        )),
        intermediate_samplerate,
    );

    chain
}

fn construct_lq_filter_chain(
    clock_rate: f32,
    target_sample_rate: f32,
    filter_type: FilterType,
) -> FilterChain {
    // https://wiki.nesdev.org/w/index.php?title=APU_Mixer

    // Quicker and more dirty. Will sound somewhat muffled.
    let mut chain = FilterChain::new();
    let cutoff_frequency = target_sample_rate * 0.40;

    chain.add(
        Box::new(LowPassIIR::new(clock_rate, cutoff_frequency)),
        clock_rate,
    );

    match filter_type {
        FilterType::Nes => {
            //The NES hardware follows the DACs with a surprisingly involved circuit that adds several low-pass and high-pass filters:

            // A first-order high-pass filter at 90 Hz
            chain.add(
                Box::new(HighPassIIR::new(target_sample_rate, 90.0)),
                target_sample_rate,
            );
            //  Another first-order high-pass filter at 440 Hz
            chain.add(
                Box::new(HighPassIIR::new(target_sample_rate, 440.0)),
                target_sample_rate,
            );
            // A first-order low-pass filter at 14 kHz
            chain.add(
                Box::new(LowPassIIR::new(target_sample_rate, 14000.0)),
                target_sample_rate,
            );
        }
        FilterType::FamiCom => {
            // The Famicom hardware instead ONLY specifies a first-order high-pass filter at 37 Hz,
            // followed by the unknown (and varying) properties of the RF modulator and demodulator.
            chain.add(
                Box::new(HighPassIIR::new(target_sample_rate, 37.0)),
                target_sample_rate,
            );
        }
    }
    chain
}
