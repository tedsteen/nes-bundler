use super::MAX_PLAYERS;
use crate::input::{gamepad::JoypadGamepadMapping, InputConfiguration, InputId, Inputs};
use core::fmt;
use serde::{Deserialize, Deserializer, Serialize};
use std::{cell::RefCell, collections::HashMap, hash::Hash, rc::Rc};

pub type InputConfigurationRef = Rc<RefCell<InputConfiguration>>;

#[derive(Debug, Clone)]
pub struct InputSettings {
    pub selected: [InputConfigurationRef; MAX_PLAYERS],
    pub configurations: HashMap<InputId, InputConfigurationRef>,
    pub default_gamepad_mapping: JoypadGamepadMapping,
}

impl InputSettings {
    pub fn get_or_create_config(
        &mut self,
        id: InputId,
        default: InputConfiguration,
    ) -> &InputConfigurationRef {
        self.configurations
            .entry(id)
            .or_insert_with(|| Rc::new(RefCell::new(default)))
    }

    pub(crate) fn reset_selected_disconnected_inputs(&mut self, inputs: &Inputs) {
        if !inputs.is_connected(&self.selected[0].borrow()) {
            self.selected[0] = Rc::clone(inputs.get_default_conf(0));
        }
        if !inputs.is_connected(&self.selected[1].borrow()) {
            self.selected[1] = Rc::clone(inputs.get_default_conf(1));
        }
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

#[derive(Serialize, Deserialize)]
struct SerializableInputSettings {
    selected: [InputId; MAX_PLAYERS],
    configurations: HashMap<InputId, InputConfiguration>,
    pub default_gamepad_mapping: JoypadGamepadMapping,
}

impl SerializableInputSettings {
    fn new(source: &InputSettings) -> Self {
        SerializableInputSettings {
            selected: source.selected.clone().map(|v| v.borrow().id.clone()),
            configurations: source
                .configurations
                .iter()
                .map(|(k, v)| (k.clone(), v.borrow().clone()))
                .collect(),
            default_gamepad_mapping: source.default_gamepad_mapping,
        }
    }
}

impl Serialize for InputSettings {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        SerializableInputSettings::new(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for InputSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        SerializableInputSettings::deserialize(deserializer)
            .and_then(|s| InputSettings::from::<D>(s))
    }
}

impl<'de> InputSettings {
    fn from<D>(source: SerializableInputSettings) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let configurations: HashMap<InputId, InputConfigurationRef> = source
            .configurations
            .iter()
            .map(|(k, v)| (k.clone(), Rc::new(RefCell::new(v.clone()))))
            .collect();
        Ok(Self {
            selected: [
                Rc::clone(
                    Self::map_selected(&configurations, &source.selected[0], 1)
                        .map_err(serde::de::Error::custom)?,
                ),
                Rc::clone(
                    Self::map_selected(&configurations, &source.selected[1], 2)
                        .map_err(serde::de::Error::custom)?,
                ),
            ],
            configurations,
            default_gamepad_mapping: source.default_gamepad_mapping,
        })
    }
    fn map_selected<'a>(
        configurations: &'a HashMap<String, InputConfigurationRef>,
        id: &'a InputId,
        player: usize,
    ) -> Result<&'a InputConfigurationRef, SettingsParseError> {
        #[allow(clippy::or_fun_call)]
        configurations
            .get(id)
            .ok_or(SettingsParseError::new(&format!(
                "non-existant input configuration '{id}' selected for player {player}"
            )))
    }
}

#[derive(Debug)]
struct SettingsParseError {
    details: String,
}

impl SettingsParseError {
    fn new(msg: &str) -> SettingsParseError {
        SettingsParseError {
            details: msg.to_string(),
        }
    }
}

impl fmt::Display for SettingsParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

impl std::error::Error for SettingsParseError {
    fn description(&self) -> &str {
        &self.details
    }
}
