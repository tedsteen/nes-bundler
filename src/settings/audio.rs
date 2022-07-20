use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub(crate) struct AudioSettings {
    pub(crate) latency: u16,
}
