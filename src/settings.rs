use std::collections::HashMap;

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

    pub(crate) fn get_p1_config(&mut self) -> &mut InputConfiguration {
        let default = Settings::default_p1_conf();
        let id = self.selected_inputs[0].get_or_insert(default.id.clone()).clone();
                
        let config = self.input_configurations.entry(id).or_insert(default);
        if config.disconnected {
            self.selected_inputs[0] = Some(DEFAULT_P1_INPUT_ID.to_string());
            config //TODO: This will result in a disconnected config for one tick.
        } else {
            config
        }
    }
    pub(crate) fn get_p2_config(&mut self) -> &mut InputConfiguration {
        let default = Settings::default_p2_conf();
        let id = self.selected_inputs[1].get_or_insert(default.id.clone()).clone();
                
        let config = self.input_configurations.entry(id).or_insert(default);
        if config.disconnected {
            self.selected_inputs[1] = Some(DEFAULT_P2_INPUT_ID.to_string());
            config //TODO: This will result in a disconnected config for one tick.
        } else {
            config
        }
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