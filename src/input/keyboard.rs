use super::{JoypadMapping, JoypadState, KeyCode, KeyEvent};
use std::collections::HashSet;

pub type JoypadKeyboardMapping = JoypadMapping<KeyCode>;

pub struct Keyboards {
    pub pressed_keys: HashSet<KeyCode>,
}

impl Keyboards {
    pub fn new() -> Self {
        Keyboards {
            pressed_keys: HashSet::new(),
        }
    }
    pub fn advance(&mut self, key_event: &KeyEvent) {
        match key_event {
            KeyEvent::Pressed(key) => {
                self.pressed_keys.insert(*key);
            }
            KeyEvent::Released(key) => {
                self.pressed_keys.remove(key);
            }
            _ => (),
        };
    }

    pub fn get_joypad(&mut self, mapping: &JoypadKeyboardMapping) -> JoypadState {
        mapping.calculate_state(&self.pressed_keys)
    }
}
