use std::{collections::{HashMap, hash_map::DefaultHasher}, rc::Rc, cell::RefCell, hash::{Hash, Hasher}};

use serde::{Serialize, Deserialize, Deserializer};

use crate::input::{InputConfiguration, InputId, keyboard::{Keyboards}};

pub(crate) const MAX_PLAYERS: usize = 2;
pub(crate) type InputConfigurationRef = Rc<RefCell<InputConfiguration>>;

#[derive(Debug)]
pub(crate) struct InputSettings {
    pub(crate) selected: [InputConfigurationRef; MAX_PLAYERS],
    pub(crate) configurations: HashMap<InputId, InputConfigurationRef>
}

#[derive(Serialize, Deserialize)]
struct SerializableInputSettings {
    selected: [InputId; MAX_PLAYERS],
    configurations: HashMap<InputId, InputConfiguration>
}
impl SerializableInputSettings {
    fn new(source: &InputSettings) -> Self {
        SerializableInputSettings {
            selected: source.selected.clone().map(|v| v.borrow().id.clone()),
            configurations: source.configurations.iter().map(|(k, v)| (k.clone(), v.borrow().clone())).collect()
        }
    }
}
impl Serialize for InputSettings {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
        SerializableInputSettings::new(self).serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for InputSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de> {
        SerializableInputSettings::deserialize(deserializer).map(InputSettings::from)
    }
}
impl Hash for InputSettings {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.selected[0].borrow().hash(state);
        self.selected[1].borrow().hash(state);
        
        for (k, v) in &self.configurations {
            k.hash(state);
            v.borrow().hash(state);
        }
    }
}

impl InputSettings {
    fn from(source: SerializableInputSettings) -> Self {
        let configurations: HashMap<InputId, InputConfigurationRef> = source.configurations.iter().map(|(k, v)| (k.clone(), Rc::new(RefCell::new(v.clone())))).collect();
        let selected = [
            Rc::clone(configurations.get(&source.selected[0]).expect("non-existant configuration selected")),
            Rc::clone(configurations.get(&source.selected[1]).expect("non-existant configuration selected"))
        ];
        Self {
            selected,
            configurations,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Hash)]
pub(crate) struct AudioSettings {
    pub(crate) latency: u16
}

#[derive(Debug, Serialize, Deserialize, Hash)]
pub(crate) struct Settings {
    pub(crate) audio: AudioSettings,
    pub(crate) input: InputSettings
}

impl InputSettings {
    pub(crate) fn get_or_create_config(&mut self, id: &InputId, default: InputConfiguration) -> &InputConfigurationRef {
        self.configurations.entry(id.clone()).or_insert_with(|| Rc::new(RefCell::new(default)))
    }
    pub(crate) fn get_default_config(&mut self, player: usize) -> &InputConfigurationRef {
        let default = Keyboards::default_configurations(player);
        self.get_or_create_config(&default.id.clone(), default)
    }
}

impl Default for Settings {
    fn default() -> Self {
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
    }
}
impl Settings {


pub(crate) fn get_hash(&self) -> u64 {
    let hasher = &mut DefaultHasher::new();
    self.hash(hasher);
    hasher.finish()
}
}