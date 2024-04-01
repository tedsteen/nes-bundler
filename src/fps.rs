use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

pub struct RateCounter {
    window: Duration,
    next_report: Instant,
    counters: HashMap<String, u64>,
}

impl RateCounter {
    pub fn new() -> Self {
        let window = Duration::from_secs(1);
        Self {
            window,
            next_report: Self::calc_next_report(&window),
            counters: HashMap::new(),
        }
    }
    fn calc_next_report(window: &Duration) -> Instant {
        Instant::now()
            .checked_add(*window)
            .expect("report instance to compute?..")
    }
    pub fn tick(&mut self, name: &str) -> &mut Self {
        *self.counters.entry(name.to_string()).or_insert(0) += 1;
        self
    }

    pub fn report(&mut self) -> Option<String> {
        if Instant::now().ge(&self.next_report) {
            self.next_report = Self::calc_next_report(&self.window);
            let window_in_sec = self.window.as_secs_f32();

            let mut res = Vec::from_iter(self.counters.iter());
            res.sort_by_key(|(key, _)| key.len());

            let res = res.iter().fold("".to_string(), |a, (name, &value)| {
                format!("{a}{name}={} ", value as f32 / window_in_sec)
            });
            self.counters.clear();
            Some(res)
        } else {
            None
        }
    }
}
