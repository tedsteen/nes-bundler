use self::{audio::AudioSettings, input::InputSettings};
use serde::{Deserialize, Serialize};
use std::{
    collections::{hash_map::DefaultHasher},
    fs::File,
    hash::{Hash, Hasher},
    io::{BufReader, BufWriter},
};

mod audio;
pub(crate) mod input;

pub(crate) const MAX_PLAYERS: usize = 2;

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub(crate) struct Settings {
    pub(crate) audio: AudioSettings,
    pub(crate) input: InputSettings,
}

impl Settings {
    pub(crate) fn new(default: &Settings) -> Self {
        Settings::load().unwrap_or_else(|_| default.clone())
    }
}

impl Settings {
    fn load() -> anyhow::Result<Settings> {
        let file = File::open("settings.json")?;
        let settings = serde_json::from_reader(BufReader::new(file))?;
        Ok(settings)
    }
    pub(crate) fn save(&self) -> anyhow::Result<()> {
        let file = File::create("settings.json")?;
        serde_json::to_writer(BufWriter::new(file), &self)?;
        Ok(())
    }

    pub(crate) fn get_hash(&self) -> u64 {
        let hasher = &mut DefaultHasher::new();
        self.hash(hasher);
        hasher.finish()
    }
}
