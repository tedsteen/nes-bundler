pub trait TimeTrait: Copy {
    fn now() -> Self;
    fn sub(&self, other: &Self) -> f64;
}

use std::time::Instant;

#[derive(Copy, Clone)]
pub struct Time(Instant);

impl TimeTrait for Time {
    fn now() -> Self {
        Self(Instant::now())
    }

    fn sub(&self, other: &Self) -> f64 {
        self.0.duration_since(other.0).as_secs_f64()
    }
}
