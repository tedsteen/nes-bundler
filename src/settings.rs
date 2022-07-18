use std::collections::{HashMap};

use crate::input::{InputConfiguration, InputId, keyboard::{JoypadKeyboardKeyMap}, InputConfigurationKind};

pub(crate) const MAX_PLAYERS: usize = 2;

#[derive(Debug)]
pub(crate) struct InputSettings {
    pub(crate) selected: [Option<InputId>; MAX_PLAYERS],
    pub(crate) configurations: HashMap<InputId, InputConfiguration>
}
#[derive(Debug)]
pub(crate) struct AudioSettings {
    pub(crate) latency: u16
}

#[derive(Debug)]
pub(crate) struct Settings {
    pub(crate) audio: AudioSettings,
    pub(crate) input: InputSettings
}

use winit::event::VirtualKeyCode::*;

impl InputSettings {
    fn default_configs() -> [InputConfiguration; MAX_PLAYERS] {
        [
            InputConfiguration { name: "Keyboard mapping #1".to_string(), id: "00-keyboard-1".to_string(), disconnected: false, kind: InputConfigurationKind::Keyboard(JoypadKeyboardKeyMap {
                up: Some(Up), down: Some(Down), left: Some(Left), right: Some(Right),
                start: Some(Return), select: Some(RShift),
                b: Some(Key1), a: Some(Key2)
            })},
            InputConfiguration { name: "Keyboard mapping #2".to_string(), id: "00-keyboard-2".to_string(), disconnected: false, kind: InputConfigurationKind::Keyboard(JoypadKeyboardKeyMap {
                up: Some(W), down: Some(S), left: Some(A), right: Some(D),
                start: Some(Key9), select: Some(Key0),
                b: Some(LAlt), a: Some(LControl)
            })}
        ]
    }

    pub(crate) fn get_config(&mut self, player: usize) -> &mut InputConfiguration {
        let default = InputSettings::default_configs()[player].clone();
        let mut id = self.selected[player].get_or_insert(default.id.clone()).clone();

        //Make sure we switch to default if it's disconnected.
        if let Some(config) = self.configurations.get(&id) {
            if config.disconnected {
                id = default.id.clone();
                self.selected[player] = Some(id.clone());
            }
        }

        self.get_or_create_config(&id, default)
    }

    pub(crate) fn get_or_create_config(&mut self, id: &InputId, default: InputConfiguration) -> &mut InputConfiguration {
        self.configurations.entry(id.clone()).or_insert(default)
    }
}

impl Default for Settings {
    fn default() -> Self {
        let audio = AudioSettings {
            latency: 40
        };
        
        let input = InputSettings {
            selected: [None, None],
            configurations: HashMap::new()
        };

        Self { audio, input }
    }
}