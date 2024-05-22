use super::MAX_PLAYERS;
use crate::input::{gamepad::JoypadGamepadMapping, InputConfiguration, InputId, Inputs};
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

    pub fn get_selected_configuration(&self, idx: usize) -> &InputConfiguration {
        self.configurations.get(&self.selected[idx]).unwrap()
    }
    pub fn get_selected_configuration_mut(&mut self, idx: usize) -> &mut InputConfiguration {
        self.configurations.get_mut(&self.selected[idx]).unwrap()
    }

    pub(crate) fn reset_selected_disconnected_inputs(&mut self, inputs: &Inputs) {
        let input_conf = self.get_selected_configuration(0);
        if !inputs.is_connected(input_conf) {
            self.selected[0].clone_from(&inputs.get_default_conf(0).id);
        }

        let input_conf = self.get_selected_configuration(1);
        if !inputs.is_connected(input_conf) {
            self.selected[1].clone_from(&inputs.get_default_conf(1).id);
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
