use std::time::{Duration, Instant};

pub struct AudioStat {
    pub latency: usize,
    birth: Instant,
}
impl AudioStat {
    pub fn new(latency: usize) -> Self {
        Self {
            latency,
            birth: Instant::now(),
        }
    }
}
pub struct AudioStats {
    pub stats: Vec<AudioStat>,
}

impl AudioStats {
    pub fn new() -> Self {
        Self {
            stats: Vec::with_capacity(1000),
        }
    }

    pub fn push_stat(&mut self, stat: AudioStat) {
        let v = &mut self.stats;
        if v.last().map_or(false, |e| {
            e.birth
                .duration_since(Instant::now())
                .ge(&Duration::from_secs(2))
        }) {
            v.pop();
        }
        self.stats.push(stat);
    }
}
