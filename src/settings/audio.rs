use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Hash)]
pub(crate) struct AudioSettings {
    pub(crate) latency: u16,
}
