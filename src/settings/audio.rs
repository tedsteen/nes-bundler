use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct AudioSettings {
    pub latency: u8,
    pub volume: u8,
}
