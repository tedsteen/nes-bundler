use crate::{
    audio::AudioSettings,
    bundle,
    input::{settings::InputSettings, InputConfigurationKind},
};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::hash_map::DefaultHasher,
    fs::File,
    hash::{Hash, Hasher},
    io::{BufReader, BufWriter},
};
pub mod gui;

pub const MAX_PLAYERS: usize = 2;

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct Settings {
    pub audio: AudioSettings,
    pub input: InputSettings,
    pub netplay_id: Option<String>,
    pub last_save_state: Option<String>,
}

impl Settings {
    pub fn load() -> Settings {
        let bundle = bundle();
        let settings_path = &bundle.settings_path;
        let default_settings = bundle.config.default_settings.clone();

        let mut settings: Result<Settings> = File::open(settings_path)
            .map_err(anyhow::Error::msg)
            .and_then(|f| serde_yaml::from_reader(BufReader::new(f)).map_err(anyhow::Error::msg));

        match &mut settings {
            Ok(settings) => {
                let default_selected = default_settings.clone().input.selected;
                //Make sure no gamepads are selected after loading settings (they will be autoselected later if they are connected)
                if let InputConfigurationKind::Gamepad(_) =
                    &settings.input.get_selected_configuration(0).kind
                {
                    settings.input.selected[0] = default_selected[0].clone();
                }
                if let InputConfigurationKind::Gamepad(_) =
                    &settings.input.get_selected_configuration(1).kind
                {
                    settings.input.selected[1] = default_selected[1].clone();
                }
            }
            Err(e) => log::warn!("Could not load settings ({:?}): {:?}", settings_path, e),
        }
        //TODO: Check if the error is something else than file not found and log
        //eprintln!("Failed to load config ({err}), falling back to default settings");
        settings.unwrap_or(default_settings)
    }
    pub fn save(&self) {
        let settings_path = &bundle().settings_path;
        if let Err(e) = File::create(settings_path)
            .map_err(anyhow::Error::msg)
            .and_then(|file| {
                serde_yaml::to_writer(BufWriter::new(file), self).map_err(anyhow::Error::msg)
            })
        {
            log::error!("Failed to save settings: {:?}", e);
        } else {
            log::debug!("Settings saved");
        }
    }

    pub fn get_hash(&self) -> u64 {
        let hasher = &mut DefaultHasher::new();
        self.hash(hasher);
        hasher.finish()
    }
}
