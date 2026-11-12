//! The audio buffer between the emulator and the sound card.
//!
//! The emulator is paced by the display, the sound card by its own clock, and the two never quite
//! agree. Instead of making either wait, the emulator keeps this buffer at its target level with
//! dynamic rate control: every frame it asks [`AudioProducer::rate_control`] how much to stretch
//! the audio, and has tetanes make a fraction of a percent more or fewer samples. See Arntzen,
//! "Dynamic Rate Control for Retro Game Emulators" (2012).
//!
//! What it steers by is the margin: the least audio left in the buffer after the sound card takes
//! some. The emulator adds a frame's worth at a time and sound cards take theirs in bursts of their
//! own size, so the level swings; keeping the low point at the latency setting keeps as little
//! buffered as the device allows, whatever its burst size.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use ringbuf::{
    HeapCons, HeapProd, HeapRb,
    traits::{Consumer, Observer, Producer, Split},
};

use crate::audio::MAX_AUDIO_LATENCY_MICROS;

/// How far rate control may stretch the audio either way, on top of what's known up front (see
/// [`AudioProducer::set_display_speed`]). Arntzen's 0.5% is inaudible and covers any real clock
/// mismatch.
const MAX_RATE_DEVIATION: f32 = 0.005;

/// Frames over which the slow part of rate control makes up a lasting difference between the
/// clocks (a minute at 60 fps). Steering by the margin alone would settle short of the target, by
/// as much as it takes to make up that difference. Slow, because what's left of the difference
/// once the display's speed is accounted for is the sound card's crystal: a few hundredths of a
/// percent.
const INTEGRAL_FRAMES: f32 = 3600.0;

/// Frames to measure the level's swing over (3 seconds at 60 fps). How many of the sound card's
/// bursts fall between two frames varies, and now and then a frame comes a display refresh late;
/// the window has to include the worst case, or the buffer gets trimmed until that case runs it
/// dry.
const SWING_WINDOW: usize = 180;

/// Frames to smooth the level over. Short, so rate control sees a change while it's happening;
/// steering by the window's low point instead would see recoveries seconds late and overshoot.
const LEVEL_FRAMES: f32 = 15.0;

/// A margin this far over the target (after a stall, say) is dropped rather than played out,
/// which at 0.5% would take seconds per millisecond.
const RESYNC_EXCESS_MICROS: u32 = 25_000;

/// Requests closer together than this are one burst: the sound card fetching its period's worth
/// in pieces.
const BURST_GAP: Duration = Duration::from_millis(2);

/// Samples to fade in over when playback starts again, so it doesn't click.
const FADE_IN_SAMPLES: usize = 88;
/// How fast the last sample fades to silence when the buffer runs dry.
const FADE_OUT: f32 = 0.99;

/// This many dropouts within `STUTTER_WITHIN` is stuttering: too little latency for this computer,
/// rather than a one-off hiccup (the window opening, say).
const STUTTER_DROPOUTS: usize = 3;
const STUTTER_WITHIN: Duration = Duration::from_secs(30);
/// A dropout counts towards stuttering only if the emulator was adding audio on time: within this
/// many frames' worth of its last addition, which allows for a frame a 120 Hz refresh late. Later
/// than that it missed a frame (skipped waiting for the other player in netplay, a long rollback,
/// a stall), which more latency wouldn't fix.
const ON_TIME_FRAMES: f32 = 1.75;

struct Shared {
    /// The margin to keep: the configured latency, in samples.
    target: AtomicUsize,
    /// The least left in the buffer after a request, since the emulator last looked.
    low_water: AtomicUsize,
    /// The most the sound card has taken in one burst.
    max_burst: AtomicUsize,
    /// Bumped to drop what's buffered (after a reset, say); the consumer clears on a change.
    flush_generation: AtomicU64,
    /// Samples for the consumer to drop, to bring an overfull buffer back to its target.
    skip: AtomicUsize,
    /// Times the sound card asked for samples the buffer didn't have.
    underruns: AtomicU64,
    /// When the sound last cut out while stuttering (see `STUTTER_DROPOUTS`), in microseconds
    /// since `epoch`; 0 for never.
    last_stutter_micros: AtomicU64,
    /// Samples in and out, for logging.
    pushed: AtomicU64,
    played: AtomicU64,
    /// When the emulator last added samples (microseconds since `epoch`), and how many: a frame's
    /// worth.
    last_push_micros: AtomicU64,
    last_push_samples: AtomicUsize,
    epoch: Instant,
    sample_rate: f32,
}

/// How the buffer is doing, for logging.
pub struct AudioStats {
    pub rate: f32,
    /// The margin last measured, and the one aimed for.
    pub margin_ms: f32,
    pub target_ms: f32,
    /// The sound card's largest burst, which the buffer has to hold on top of the margin.
    pub burst_ms: f32,
    /// The lasting clock difference rate control has learned, as a ratio.
    pub drift: f32,
    pub underruns: u64,
    pub pushed: u64,
    pub played: u64,
}

pub struct AudioProducer {
    tx: HeapProd<f32>,
    shared: Arc<Shared>,
    /// The low point of each of the last frames, oldest overwritten first, and how many of them
    /// have been measured.
    low_points: [usize; SWING_WINDOW],
    next_low_point: usize,
    measured: usize,
    /// The low points, smoothed.
    level: f32,
    /// The margin rate control steers by: the smoothed level less its swing.
    margin: f32,
    /// How much faster or slower than its own clock the display runs the game, as a ratio.
    display_speed: f32,
    /// The lasting difference between the clocks, learned by the slow part of rate control.
    drift: f32,
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
            .last_push_samples
            .store(samples.len(), Ordering::Relaxed);
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

    /// The display runs the game `speed` times as fast as its own frame rate (a 120 Hz display
    /// running NTSC's 60.1 fps at 60, say). Known up front, so it's made up for directly, leaving
    /// rate control only the sound card's clock.
    pub fn set_display_speed(&mut self, speed: f32) {
        self.display_speed = speed;
    }

    /// The ratio to make audio at: above 1 when the margin runs low, below 1 when it runs high.
    /// Call it once per frame, before making the frame's samples.
    pub fn rate_control(&mut self) -> f32 {
        let target = self.shared.target.load(Ordering::Relaxed) as f32;
        let low_water = self.shared.low_water.swap(usize::MAX, Ordering::Relaxed);
        if low_water != usize::MAX {
            self.measure(low_water, target);
        }

        let error = ((target - self.margin) / target).clamp(-1.0, 1.0);
        self.drift = (self.drift + MAX_RATE_DEVIATION * error / INTEGRAL_FRAMES)
            .clamp(-MAX_RATE_DEVIATION, MAX_RATE_DEVIATION);
        let correction = (MAX_RATE_DEVIATION * error + self.drift)
            .clamp(-MAX_RATE_DEVIATION, MAX_RATE_DEVIATION);
        self.last_rate = (1.0 + correction) / self.display_speed;
        self.last_rate
    }

    fn measure(&mut self, low_water: usize, target: f32) {
        self.low_points[self.next_low_point] = low_water;
        self.next_low_point = (self.next_low_point + 1) % SWING_WINDOW;
        self.measured = (self.measured + 1).min(SWING_WINDOW);
        self.level = if self.measured == 1 {
            low_water as f32
        } else {
            self.level + (low_water as f32 - self.level) / LEVEL_FRAMES
        };

        // How far below its average the level dips, which the margin has to cover. The window
        // isn't in order, but neither the lowest nor the average care.
        let window = &self.low_points[..self.measured];
        let lowest = window.iter().copied().min().unwrap_or(low_water) as f32;
        let average = window.iter().sum::<usize>() as f32 / window.len() as f32;
        self.margin = self.level - (average - lowest);

        let excess = self.margin - target;
        if self.measured == SWING_WINDOW
            && excess > samples_for(RESYNC_EXCESS_MICROS, self.shared.sample_rate) as f32
        {
            log::debug!(
                "Audio buffer {:.1} ms over its target, dropping the excess",
                excess / self.shared.sample_rate * 1000.0
            );
            self.shared.skip.store(excess as usize, Ordering::Relaxed);
            self.measured = 0;
            self.next_low_point = 0;
            self.margin = target;
        }
    }

    pub fn flush_handle(&self) -> AudioFlush {
        AudioFlush(Arc::clone(&self.shared))
    }

    pub fn stats(&self) -> AudioStats {
        let ms = |samples: f32| samples / self.shared.sample_rate * 1000.0;
        AudioStats {
            rate: self.last_rate,
            margin_ms: ms(self.margin),
            target_ms: ms(self.shared.target.load(Ordering::Relaxed) as f32),
            burst_ms: ms(self.shared.max_burst.load(Ordering::Relaxed) as f32),
            drift: self.drift,
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
    /// Waiting for the buffer to refill, at startup and after running dry, so playback restarts
    /// with room for a burst plus the margin instead of stuttering on every request.
    refilling: bool,
    last_request: Option<Instant>,
    burst: usize,
    /// The last sample played, to fade out from when the buffer runs dry.
    last_sample: f32,
    /// Samples into the fade-in after a refill.
    faded_in: usize,
    /// When the sound cut out the last few times before (microseconds since `epoch`), oldest
    /// overwritten first.
    dropouts: [Option<u64>; STUTTER_DROPOUTS - 1],
    next_dropout: usize,
}

impl AudioConsumer {
    /// Fills `out` with buffered samples, or silence while refilling. Returns how many samples
    /// came from the buffer.
    pub fn pop_slice(&mut self, out: &mut [f32]) -> usize {
        let generation = self.shared.flush_generation.load(Ordering::Acquire);
        if generation != self.observed_generation {
            self.observed_generation = generation;
            self.rx.clear();
            self.start_refill();
        }
        let skip = self.shared.skip.swap(0, Ordering::Relaxed);
        if skip > 0 {
            self.rx.skip(skip.min(self.rx.occupied_len()));
        }

        let now = Instant::now();
        let in_burst = self
            .last_request
            .is_some_and(|last| now.duration_since(last) < BURST_GAP);
        self.last_request = Some(now);
        self.burst = if in_burst { self.burst } else { 0 } + out.len();
        self.shared
            .max_burst
            .fetch_max(self.burst, Ordering::Relaxed);

        if self.refilling {
            let refill_to = self.shared.target.load(Ordering::Relaxed)
                + self.shared.max_burst.load(Ordering::Relaxed);
            if self.rx.occupied_len() < refill_to {
                self.fade_out(out);
                return 0;
            }
            self.refilling = false;
        }

        let got = self.rx.pop_slice(out);
        self.shared
            .played
            .fetch_add(out.len() as u64, Ordering::Relaxed);
        self.shared
            .low_water
            .fetch_min(self.rx.occupied_len(), Ordering::Relaxed);
        for sample in &mut out[..got] {
            if self.faded_in < FADE_IN_SAMPLES {
                self.faded_in += 1;
                *sample *= self.faded_in as f32 / FADE_IN_SAMPLES as f32;
            }
            self.last_sample = *sample;
        }
        if got < out.len() {
            self.fade_out(&mut out[got..]);
            self.shared.underruns.fetch_add(1, Ordering::Relaxed);
            let now = self.shared.epoch.elapsed().as_micros() as u64;
            let since_push =
                now.saturating_sub(self.shared.last_push_micros.load(Ordering::Relaxed));
            let frame_micros = self.shared.last_push_samples.load(Ordering::Relaxed) as f32
                / self.shared.sample_rate
                * 1_000_000.0;
            let emulator_on_time = since_push as f32 <= frame_micros * ON_TIME_FRAMES;
            if emulator_on_time {
                self.record_dropout(now);
            }
            log::debug!(
                "Audio buffer ran dry ({got} of {} samples), {:.1} ms after the emulator last added audio{}",
                out.len(),
                since_push as f32 / 1000.0,
                if emulator_on_time {
                    ""
                } else {
                    " (the emulator fell behind)"
                }
            );
            self.start_refill();
        }
        got
    }

    fn record_dropout(&mut self, now: u64) {
        // The oldest of the ones before, overwritten by this one.
        let oldest = self.dropouts[self.next_dropout].replace(now);
        self.next_dropout = (self.next_dropout + 1) % self.dropouts.len();
        if oldest.is_some_and(|oldest| now - oldest <= STUTTER_WITHIN.as_micros() as u64) {
            self.shared
                .last_stutter_micros
                .store(now.max(1), Ordering::Relaxed);
        }
    }

    fn start_refill(&mut self) {
        self.refilling = true;
        self.faded_in = 0;
    }

    /// Fills `out` with the last sample fading to silence, which clicks less than stopping dead.
    fn fade_out(&mut self, out: &mut [f32]) {
        for sample in out {
            self.last_sample *= FADE_OUT;
            *sample = self.last_sample;
        }
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

    /// How long ago the sound last cut out while stuttering: several times in a short while,
    /// which more latency would fix.
    pub fn since_stutter(&self) -> Option<Duration> {
        let last_stutter = self.shared.last_stutter_micros.load(Ordering::Relaxed);
        (last_stutter > 0).then(|| {
            self.shared
                .epoch
                .elapsed()
                .saturating_sub(Duration::from_micros(last_stutter))
        })
    }
}

fn samples_for(micros: u32, sample_rate: f32) -> usize {
    ((micros as f32 / 1_000_000.0) * sample_rate)
        .round()
        .max(1.0) as usize
}

/// `callback_frames` is the most the sound card asks for in one request; it may make several
/// requests in a burst.
pub fn audio_buffer(
    latency_micros: u32,
    sample_rate: f32,
    callback_frames: usize,
) -> (AudioProducer, AudioConsumer, AudioBufferControl) {
    // The largest margin, the excess allowed over it before it's dropped, two frames (at PAL's
    // 50 fps) of swing as frames are added, and room for a sound card taking up to 25 ms at a time.
    let capacity = samples_for(
        MAX_AUDIO_LATENCY_MICROS + RESYNC_EXCESS_MICROS + 2 * 20_000 + 25_000,
        sample_rate,
    );
    let (tx, rx) = HeapRb::<f32>::new(capacity).split();
    let shared = Arc::new(Shared {
        target: AtomicUsize::new(samples_for(latency_micros, sample_rate)),
        low_water: AtomicUsize::new(usize::MAX),
        // Until the sound card shows what it takes, assume one request.
        max_burst: AtomicUsize::new(callback_frames),
        flush_generation: AtomicU64::new(0),
        skip: AtomicUsize::new(0),
        underruns: AtomicU64::new(0),
        last_stutter_micros: AtomicU64::new(0),
        pushed: AtomicU64::new(0),
        played: AtomicU64::new(0),
        last_push_micros: AtomicU64::new(0),
        last_push_samples: AtomicUsize::new(0),
        epoch: Instant::now(),
        sample_rate,
    });
    (
        AudioProducer {
            tx,
            shared: Arc::clone(&shared),
            low_points: [0; SWING_WINDOW],
            next_low_point: 0,
            measured: 0,
            level: 0.0,
            margin: 0.0,
            display_speed: 1.0,
            drift: 0.0,
            last_rate: 1.0,
        },
        AudioConsumer {
            rx,
            shared: Arc::clone(&shared),
            observed_generation: 0,
            refilling: true,
            last_request: None,
            burst: 0,
            last_sample: 0.0,
            faded_in: 0,
            dropouts: [None; STUTTER_DROPOUTS - 1],
            next_dropout: 0,
        },
        AudioBufferControl {
            shared,
            sample_rate,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: u64 = 1_000_000;

    #[test]
    fn a_few_scattered_dropouts_are_not_stuttering() {
        let (_tx, mut rx, ctl) = audio_buffer(8_000, 44_100.0, 128);
        for second in [1, 20, 40, 60, 80] {
            rx.record_dropout(second * SECOND);
        }
        assert!(ctl.since_stutter().is_none());
    }

    #[test]
    fn dropouts_close_together_are_stuttering() {
        let (_tx, mut rx, ctl) = audio_buffer(8_000, 44_100.0, 128);
        rx.record_dropout(SECOND);
        rx.record_dropout(10 * SECOND);
        assert!(ctl.since_stutter().is_none());
        rx.record_dropout(20 * SECOND);
        assert!(ctl.since_stutter().is_some());
    }
}
