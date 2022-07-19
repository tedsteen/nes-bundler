use std::{collections::{HashMap, hash_map::DefaultHasher}, rc::Rc, cell::RefCell, hash::{Hash, Hasher}, fs::File, io::{BufWriter, BufReader}};
use serde::{Serialize, Deserialize};
use crate::input::{keyboard::{Keyboards}};
use self::{audio::AudioSettings, input::InputSettings};

pub(crate) mod input;
mod audio;

pub(crate) const MAX_PLAYERS: usize = 2;

#[derive(Debug, Serialize, Deserialize, Hash)]
pub(crate) struct Settings {
    pub(crate) audio: AudioSettings,
    pub(crate) input: InputSettings
}

impl Default for Settings {
    fn default() -> Self {
        Settings::load_settings().unwrap_or_else(|_| {
            let audio = AudioSettings {
                latency: 40
            };
            let default_input_1 = Rc::new(RefCell::new(Keyboards::default_configurations(0)));
            let default_input_2 = Rc::new(RefCell::new(Keyboards::default_configurations(1)));
    
            let mut configurations = HashMap::new();
            configurations.insert(default_input_1.borrow().id.clone(), default_input_1.clone());
            configurations.insert(default_input_2.borrow().id.clone(), default_input_2.clone());
    
            let selected = [
                Rc::clone(&default_input_1),
                Rc::clone(&default_input_2),
            ];
    
            let input = InputSettings {
                selected,
                configurations
            };
    
            Self { audio, input }
        })
    }
}

impl Settings {
    fn load_settings() -> anyhow::Result<Settings> {
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