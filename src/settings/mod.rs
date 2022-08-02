use crate::input::InputConfigurationKind;

use self::{audio::AudioSettings, input::InputSettings};
use serde::{Deserialize, Serialize};
use std::{
    collections::hash_map::DefaultHasher,
    fs::File,
    hash::{Hash, Hasher},
    io::{BufReader, BufWriter},
    rc::Rc,
};

pub mod audio;
pub mod input;

pub const MAX_PLAYERS: usize = 2;

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct Settings {
    pub audio: AudioSettings,
    pub input: InputSettings,
    pub netplay_id: Option<String>,
    pub last_save_state: Option<String>,
}

impl Settings {
    pub fn new(default_settings: &Settings) -> Self {
        let mut settings = Settings::load();
        let default_selected = &default_settings.input.selected;
        if let Ok(settings) = &mut settings {
            //Make sure no gamepads are selected after loading settings (they will be autoselected later if they are connected)
            if let InputConfigurationKind::Gamepad(_) =
                Rc::clone(&settings.input.selected[0]).borrow().kind
            {
                settings.input.selected[0] = Rc::clone(&default_selected[0]);
            }
            if let InputConfigurationKind::Gamepad(_) =
                Rc::clone(&settings.input.selected[1]).borrow().kind
            {
                settings.input.selected[1] = Rc::clone(&default_selected[1]);
            }
        }
        settings.unwrap_or_else(|err| {
            eprintln!("Failed to load config ({err}), falling back to default settings");
            default_settings.clone()
        })
    }
}

impl Settings {
    fn load() -> anyhow::Result<Settings> {
        let settings = serde_yaml::from_reader(BufReader::new(File::open("settings.yaml")?))?;
        Ok(settings)
    }
    pub fn save(&self) -> anyhow::Result<()> {
        serde_yaml::to_writer(BufWriter::new(File::create("settings.yaml")?), &self)?;
        Ok(())
    }

    pub fn get_hash(&self) -> u64 {
        let hasher = &mut DefaultHasher::new();
        self.hash(hasher);
        hasher.finish()
    }
}
