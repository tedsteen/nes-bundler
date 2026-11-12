//! Telling a persistent problem from a one-off: something that happens again and again in a short
//! while, rather than once.

use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

pub struct Recurrence {
    /// When it last happened, up to `times` of them, oldest first.
    happened: VecDeque<Instant>,
    times: usize,
    within: Duration,
}

impl Recurrence {
    /// Recurring is happening `times` times within `within`.
    pub fn new(times: usize, within: Duration) -> Self {
        Self {
            // Allocated once: recording never grows it, so it's fine on the audio thread.
            happened: VecDeque::with_capacity(times),
            times,
            within,
        }
    }

    /// Records that it happened `now`, and returns whether that makes it recurring.
    pub fn record(&mut self, now: Instant) -> bool {
        if self.happened.len() == self.times {
            self.happened.pop_front();
        }
        self.happened.push_back(now);
        self.happened.len() == self.times
            && self
                .happened
                .front()
                .is_some_and(|first| now.duration_since(*first) <= self.within)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn scattered_is_not_recurring() {
        let start = Instant::now();
        let mut recurrence = Recurrence::new(3, 30 * SECOND);
        for second in [1, 20, 40, 60, 80] {
            assert!(!recurrence.record(start + second * SECOND));
        }
    }

    #[test]
    fn close_together_is_recurring() {
        let start = Instant::now();
        let mut recurrence = Recurrence::new(3, 30 * SECOND);
        assert!(!recurrence.record(start + SECOND));
        assert!(!recurrence.record(start + 10 * SECOND));
        assert!(recurrence.record(start + 20 * SECOND));
        assert!(recurrence.record(start + 25 * SECOND));
    }
}
