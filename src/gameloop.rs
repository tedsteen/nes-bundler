pub trait TimeTrait: Copy {
    fn now() -> Self;
    fn sub(&self, other: &Self) -> f64;
}

use std::{fmt::Debug, time::Instant};

use crate::Fps;

#[derive(Debug, Copy, Clone)]
pub struct Time(Instant);

impl TimeTrait for Time {
    fn now() -> Self {
        Self(Instant::now())
    }

    fn sub(&self, other: &Self) -> f64 {
        self.0.duration_since(other.0).as_secs_f64()
    }
}

pub struct GameLoop<G> {
    pub game: G,
    pub updates_per_second: Fps,
    pub max_frame_time: f64,

    fixed_time_step: f64,
    last_frame_time: f64,
    running_time: f64,
    accumulated_time: f64,
    previous_instant: Time,
    current_instant: Time,
}

impl<G> GameLoop<G> {
    pub fn new(game: G, updates_per_second: Fps, max_frame_time: f64) -> Self {
        Self {
            game,
            updates_per_second,
            max_frame_time,

            fixed_time_step: 1.0 / updates_per_second as f64,

            running_time: 0.0,
            accumulated_time: 0.0,
            previous_instant: Time::now(),
            current_instant: Time::now(),
            last_frame_time: 0.0,
        }
    }

    pub fn next_frame<U>(&mut self, mut update: U)
    where
        U: FnMut(&mut GameLoop<G>),
    {
        let g = self;

        g.current_instant = Time::now();

        let mut elapsed = g.current_instant.sub(&g.previous_instant);

        if elapsed > g.max_frame_time {
            elapsed = g.max_frame_time;
        }

        g.last_frame_time = elapsed;
        g.running_time += elapsed;
        g.accumulated_time += elapsed;

        while g.accumulated_time >= g.fixed_time_step {
            update(g);

            g.accumulated_time -= g.fixed_time_step;
        }

        g.previous_instant = g.current_instant;
    }

    pub fn set_updates_per_second(&mut self, new_updates_per_second: Fps) {
        if self.updates_per_second != new_updates_per_second {
            self.updates_per_second = new_updates_per_second;
            self.fixed_time_step = 1.0 / new_updates_per_second as f64;
        }
    }
}
