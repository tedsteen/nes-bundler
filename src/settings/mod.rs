use self::{audio::AudioSettings, input::InputSettings};
use crate::input::keyboard::Keyboards;
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{hash_map::DefaultHasher, HashMap},
    fs::File,
    hash::{Hash, Hasher},
    io::{BufReader, BufWriter},
    rc::Rc,
};

mod audio;
pub(crate) mod input;

pub(crate) const MAX_PLAYERS: usize = 2;

#[derive(Debug, Serialize, Deserialize, Hash)]
pub(crate) struct Settings {
    pub(crate) audio: AudioSettings,
    pub(crate) input: InputSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Settings::load().unwrap_or_else(|_| {
            let audio = AudioSettings { latency: 40 };
            let default_input_1 = Rc::new(RefCell::new(Keyboards::default_configurations(0)));
            let default_input_2 = Rc::new(RefCell::new(Keyboards::default_configurations(1)));

            let mut configurations = HashMap::new();
            configurations.insert(default_input_1.borrow().id.clone(), default_input_1.clone());
            configurations.insert(default_input_2.borrow().id.clone(), default_input_2.clone());

            let selected = [Rc::clone(&default_input_1), Rc::clone(&default_input_2)];

            let input = InputSettings {
                selected,
                configurations,
            };

            Self { audio, input }
        })
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
