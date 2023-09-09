pub trait TimeTrait: Copy {
    fn now() -> Self;
    fn sub(&self, other: &Self) -> f64;
}

use std::{fmt::Debug, time::Instant};

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

pub struct GameLoop<G, T: TimeTrait> {
    pub game: G,
    pub updates_per_second: u32,
    pub max_frame_time: f64,
    pub exit_next_iteration: bool,

    fixed_time_step: f64,
    pub last_stats: T,
    updates: Vec<T>,
    renders: Vec<T>,
    last_frame_time: f64,
    running_time: f64,
    accumulated_time: f64,
    blending_factor: f64,
    previous_instant: T,
    current_instant: T,
}
const SAMPLE_WINDOW: f64 = 1.0;

impl<G, T: TimeTrait + Debug> GameLoop<G, T> {
    pub fn new(game: G, updates_per_second: u32, max_frame_time: f64) -> Self {
        Self {
            game,
            updates_per_second,
            max_frame_time,
            exit_next_iteration: false,

            fixed_time_step: 1.0 / updates_per_second as f64,

            last_stats: T::now(),
            updates: vec![],
            renders: vec![],

            running_time: 0.0,
            accumulated_time: 0.0,
            blending_factor: 0.0,
            previous_instant: T::now(),
            current_instant: T::now(),
            last_frame_time: 0.0,
        }
    }

    pub fn get_stats(&mut self) -> (f64, f64, f64, T) {
        let res = (
            self.updates.len() as f64 / SAMPLE_WINDOW,
            self.renders.len() as f64 / SAMPLE_WINDOW,
            self.running_time,
            self.last_stats,
        );
        self.last_stats = T::now();
        res
    }

    pub fn next_frame<U, R, E>(&mut self, mut update: U, mut render: R, extra: E) -> bool
    where
        U: FnMut(&mut GameLoop<G, T>, &E),
        R: FnMut(&mut GameLoop<G, T>, &E),
    {
        let g = self;

        if g.exit_next_iteration {
            return false;
        }

        g.current_instant = T::now();

        let mut elapsed = g.current_instant.sub(&g.previous_instant);

        if elapsed > g.max_frame_time {
            elapsed = g.max_frame_time;
        }

        g.last_frame_time = elapsed;
        g.running_time += elapsed;
        g.accumulated_time += elapsed;

        while g.accumulated_time >= g.fixed_time_step {
            update(g, &extra);

            g.accumulated_time -= g.fixed_time_step;
            g.updates.push(g.current_instant);
            g.updates
                .retain(|e| g.current_instant.sub(e) <= SAMPLE_WINDOW);
        }

        g.blending_factor = g.accumulated_time / g.fixed_time_step;

        render(g, &extra);

        g.renders.push(g.current_instant);
        g.renders
            .retain(|e| g.current_instant.sub(e) <= SAMPLE_WINDOW);
        g.previous_instant = g.current_instant;

        true
    }

    pub fn exit(&mut self) {
        self.exit_next_iteration = true;
    }

    pub fn set_updates_per_second(&mut self, new_updates_per_second: u32) {
        self.updates_per_second = new_updates_per_second;
        self.fixed_time_step = 1.0 / new_updates_per_second as f64;
    }
}
