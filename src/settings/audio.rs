use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct AudioSettings {
    pub latency: u16,
    #[serde(default = "default_volume")]
    pub volume: u8,
}
fn default_volume() -> u8 { 100 }
