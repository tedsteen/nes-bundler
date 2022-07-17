use std::collections::{HashMap};

use crate::input::{InputConfiguration, InputId, keyboard::{JoypadKeyboardKeyMap}, InputConfigurationKind};

pub(crate) const MAX_PLAYERS: usize = 2;

#[derive(Debug)]
pub(crate) struct Settings {
    pub(crate) audio_latency: u16,
    pub(crate) selected_inputs: [Option<InputId>; MAX_PLAYERS],
    pub(crate) input_configurations: HashMap<InputId, InputConfiguration>
}

use winit::event::VirtualKeyCode::*;

pub(crate) const DEFAULT_P1_INPUT_ID: &str = "00-keyboard-1";
pub(crate) const DEFAULT_P2_INPUT_ID: &str = "00-keyboard-2";

impl Settings {
    fn default_p1_conf() -> InputConfiguration {
        InputConfiguration { name: "Keyboard mapping #1".to_string(), id: DEFAULT_P1_INPUT_ID.to_string(), disconnected: false, kind: InputConfigurationKind::Keyboard(JoypadKeyboardKeyMap {
            up: Some(Up), down: Some(Down), left: Some(Left), right: Some(Right),
            start: Some(Return), select: Some(RShift),
            b: Some(Key1), a: Some(Key2)
        })}
    }
    fn default_p2_conf() -> InputConfiguration {
        InputConfiguration { name: "Keyboard mapping #2".to_string(), id: DEFAULT_P2_INPUT_ID.to_string(), disconnected: false, kind: InputConfigurationKind::Keyboard(JoypadKeyboardKeyMap {
            up: Some(W), down: Some(S), left: Some(A), right: Some(D),
            start: Some(Key9), select: Some(Key0),
            b: Some(LAlt), a: Some(LControl)
        })}
    }

    fn get_config(&mut self, player: usize, default: InputConfiguration) -> &mut InputConfiguration {
        let default_id = default.id.clone();
        let mut id = self.selected_inputs[player].get_or_insert(default_id.clone()).clone();

        //Make sure we switch to default if it's disconnected.
        if let Some(config) = self.input_configurations.get(&id) {
            if config.disconnected {
                id = default_id;
                self.selected_inputs[player] = Some(id.clone());
            }
        }

        self.input_configurations.entry(id).or_insert(default)
    }

    pub(crate) fn get_p1_config(&mut self) -> &mut InputConfiguration {
        self.get_config(0, Settings::default_p1_conf())
    }
    pub(crate) fn get_p2_config(&mut self) -> &mut InputConfiguration {
        self.get_config(1, Settings::default_p2_conf())
    }
    pub(crate) fn get_or_create_configuration(&mut self, id: &InputId, default: InputConfiguration) -> &mut InputConfiguration {
        self.input_configurations.entry(id.clone()).or_insert(default)
    }
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            audio_latency: 30,
            selected_inputs: [None, None],
            input_configurations: HashMap::new()
        }
    }
}