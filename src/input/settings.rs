use super::MAX_PLAYERS;
use crate::input::{InputConfiguration, InputId, Inputs, gamepad::JoypadGamepadMapping};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, hash::Hash};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSettings {
    pub selected: [InputId; MAX_PLAYERS],
    pub configurations: BTreeMap<InputId, InputConfiguration>,
    pub default_gamepad_mapping: JoypadGamepadMapping,
}

impl InputSettings {
    pub fn get_or_create_config(
        &mut self,
        id: InputId,
        default: InputConfiguration,
    ) -> &InputConfiguration {
        self.configurations.entry(id).or_insert_with(|| default)
    }

    pub fn selected_configuration(&self, idx: usize) -> &InputConfiguration {
        self.configurations.get(&self.selected[idx]).unwrap()
    }
    pub fn selected_configuration_mut(&mut self, idx: usize) -> &mut InputConfiguration {
        self.configurations.get_mut(&self.selected[idx]).unwrap()
    }

    pub(crate) fn reset_selected_disconnected_inputs(&mut self, inputs: &Inputs) {
        for player in 0..MAX_PLAYERS {
            let input_conf = self.selected_configuration(player);
            if !inputs.is_connected(input_conf) {
                self.selected[player].clone_from(&inputs.default_configuration(player).id);
            }
        }
    }
}

impl Hash for InputSettings {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.selected[0].hash(state);
        self.selected[1].hash(state);

        for (k, v) in &self.configurations {
            k.hash(state);
            v.hash(state);
        }
    }
}
