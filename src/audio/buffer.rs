//! The audio buffer between the emulator and the sound card.
//!
//! The emulator is paced by the display, the sound card by its own clock, and the two never quite
//! agree. Instead of making either wait, the emulator keeps this buffer at its target level with
//! dynamic rate control: every frame it asks [`AudioProducer::rate_control`] how much to stretch
//! the audio, and has tetanes make a fraction of a percent more or fewer samples. See Arntzen,
//! "Dynamic Rate Control for Retro Game Emulators" (2012).

use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

use ringbuf::{
    HeapCons, HeapProd, HeapRb,
    traits::{Consumer, Observer, Producer, Split},
};

use crate::audio::MAX_AUDIO_LATENCY_MICROS;

/// How far rate control may stretch the audio either way. Arntzen's 0.5% is inaudible and
/// covers any real clock mismatch, like a 60 Hz display showing 60.1 Hz NTSC.
const MAX_RATE_DEVIATION: f32 = 0.005;

struct Shared {
    /// Samples the emulator should keep buffered: the configured latency.
    target: AtomicUsize,
    /// The most the sound card takes at once. The emulator adds a frame's worth of samples at a
    /// time, so the level swings; it aims this much above the target so even the low point, just
    /// before a frame, still covers the next request.
    headroom: usize,
    /// Bumped to drop what's buffered (after a reset, say); the consumer clears on a change.
    flush_generation: AtomicU64,
    /// Times the sound card asked for samples the buffer didn't have.
    underruns: AtomicU64,
    /// Samples in and out, for logging.
    pushed: AtomicU64,
    played: AtomicU64,
    /// When the emulator last added samples (microseconds since `epoch`), for logging.
    last_push_micros: AtomicU64,
    epoch: std::time::Instant,
    sample_rate: f32,
}

/// How the buffer is doing, for logging.
pub struct AudioStats {
    pub rate: f32,
    pub buffered_ms: f32,
    pub target_ms: f32,
    pub underruns: u64,
    pub pushed: u64,
    pub played: u64,
}

pub struct AudioProducer {
    tx: HeapProd<f32>,
    shared: Arc<Shared>,
    /// The last ratio `rate_control` gave, for logging.
    last_rate: f32,
}

impl AudioProducer {
    /// Queues samples without waiting. Should the sound card stop taking them, the excess is
    /// dropped rather than holding the emulator up.
    pub fn push(&mut self, samples: &[f32]) {
        let pushed = self.tx.push_slice(samples);
        self.shared.last_push_micros.store(
            self.shared.epoch.elapsed().as_micros() as u64,
            Ordering::Relaxed,
        );
        self.shared
            .pushed
            .fetch_add(samples.len() as u64, Ordering::Relaxed);
        if pushed < samples.len() {
            log::trace!(
                "Audio buffer full, dropped {} samples",
                samples.len() - pushed
            );
        }
    }

    /// The ratio to make audio at: above 1 when the buffer runs low, below 1 when it runs high.
    /// Call it before making a frame's worth of samples.
    pub fn rate_control(&mut self) -> f32 {
        let target = self.shared.setpoint() as f32;
        let buffered = self.tx.occupied_len() as f32;
        let error = ((target - buffered) / target).clamp(-1.0, 1.0);
        self.last_rate = 1.0 + MAX_RATE_DEVIATION * error;
        self.last_rate
    }

    pub fn flush_handle(&self) -> AudioFlush {
        AudioFlush(Arc::clone(&self.shared))
    }

    pub fn stats(&self) -> AudioStats {
        let ms = |samples: usize| samples as f32 / self.shared.sample_rate * 1000.0;
        AudioStats {
            rate: self.last_rate,
            buffered_ms: ms(self.tx.occupied_len()),
            target_ms: ms(self.shared.setpoint()),
            underruns: self.shared.underruns.load(Ordering::Relaxed),
            pushed: self.shared.pushed.load(Ordering::Relaxed),
            played: self.shared.played.load(Ordering::Relaxed),
        }
    }
}

/// Drops whatever audio is buffered, for when it no longer fits what's on screen.
#[derive(Clone)]
pub struct AudioFlush(Arc<Shared>);

impl AudioFlush {
    pub fn flush(&self) {
        self.0.flush_generation.fetch_add(1, Ordering::Release);
    }
}

pub struct AudioConsumer {
    rx: HeapCons<f32>,
    shared: Arc<Shared>,
    observed_generation: u64,
    /// Waiting for the buffer to refill to its target, at startup and after running dry, so
    /// playback restarts with room to absorb jitter instead of stuttering on every sample.
    refilling: bool,
}

impl AudioConsumer {
    /// Fills `out` with buffered samples, or silence while refilling. Returns how many samples
    /// came from the buffer.
    pub fn pop_slice(&mut self, out: &mut [f32]) -> usize {
        let generation = self.shared.flush_generation.load(Ordering::Acquire);
        if generation != self.observed_generation {
            self.observed_generation = generation;
            self.rx.clear();
            self.refilling = true;
        }

        if self.refilling {
            if self.rx.occupied_len() < self.shared.setpoint() {
                out.fill(0.0);
                return 0;
            }
            self.refilling = false;
        }

        let got = self.rx.pop_slice(out);
        self.shared
            .played
            .fetch_add(out.len() as u64, Ordering::Relaxed);
        if got < out.len() {
            out[got..].fill(0.0);
            self.shared.underruns.fetch_add(1, Ordering::Relaxed);
            let since_push = self.shared.epoch.elapsed().as_micros() as u64
                - self.shared.last_push_micros.load(Ordering::Relaxed);
            log::debug!(
                "Audio buffer ran dry ({got} of {} samples), {:.1} ms after the emulator last added audio",
                out.len(),
                since_push as f32 / 1000.0
            );
            self.refilling = true;
        }
        got
    }
}

#[derive(Clone)]
pub struct AudioBufferControl {
    shared: Arc<Shared>,
    sample_rate: f32,
}

impl AudioBufferControl {
    pub fn set_latency_micros(&self, latency_micros: u32) {
        self.shared.target.store(
            samples_for(latency_micros, self.sample_rate),
            Ordering::Relaxed,
        );
    }
}

fn samples_for(micros: u32, sample_rate: f32) -> usize {
    ((micros as f32 / 1_000_000.0) * sample_rate)
        .round()
        .max(1.0) as usize
}

impl Shared {
    /// The level to keep the buffer at, measured just before the emulator adds a frame.
    fn setpoint(&self) -> usize {
        self.target.load(Ordering::Relaxed) + self.headroom
    }
}

/// `callback_frames` is the most the sound card takes at once.
pub fn audio_buffer(
    latency_micros: u32,
    sample_rate: f32,
    callback_frames: usize,
) -> (AudioProducer, AudioConsumer, AudioBufferControl) {
    // Room for the largest latency target plus a few frames of slack.
    let capacity = samples_for(MAX_AUDIO_LATENCY_MICROS, sample_rate) * 4;
    let (tx, rx) = HeapRb::<f32>::new(capacity).split();
    let shared = Arc::new(Shared {
        target: AtomicUsize::new(samples_for(latency_micros, sample_rate)),
        headroom: callback_frames,
        flush_generation: AtomicU64::new(0),
        underruns: AtomicU64::new(0),
        pushed: AtomicU64::new(0),
        played: AtomicU64::new(0),
        last_push_micros: AtomicU64::new(0),
        epoch: std::time::Instant::now(),
        sample_rate,
    });
    (
        AudioProducer {
            tx,
            shared: Arc::clone(&shared),
            last_rate: 1.0,
        },
        AudioConsumer {
            rx,
            shared: Arc::clone(&shared),
            observed_generation: 0,
            refilling: true,
        },
        AudioBufferControl {
            shared,
            sample_rate,
        },
    )
}
