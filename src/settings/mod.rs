use self::{audio::AudioSettings, input::InputSettings};
use serde::{Deserialize, Serialize};
use std::{
    collections::hash_map::DefaultHasher,
    fs::File,
    hash::{Hash, Hasher},
    io::{BufReader, BufWriter},
};

pub mod audio;
pub mod input;

pub const MAX_PLAYERS: usize = 2;

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct Settings {
    pub audio: AudioSettings,
    pub input: InputSettings,
}

impl Settings {
    pub fn new() -> anyhow::Result<Self> {
        Settings::load()
    }
}

impl Settings {
    fn load() -> anyhow::Result<Settings> {
        let file = File::open("settings.json")?;
        let settings = serde_json::from_reader(BufReader::new(file))?;
        Ok(settings)
    }
    pub fn save(&self) -> anyhow::Result<()> {
        let file = File::create("settings.json")?;
        serde_json::to_writer(BufWriter::new(file), &self)?;
        Ok(())
    }

    pub fn get_hash(&self) -> u64 {
        let hasher = &mut DefaultHasher::new();
        self.hash(hasher);
        hasher.finish()
    }
}
