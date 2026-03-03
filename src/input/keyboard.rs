use super::{JoypadMapping, JoypadState, KeyCode, KeyEvent};
use std::collections::HashSet;

pub type JoypadKeyboardMapping = JoypadMapping<KeyCode>;

#[derive(Default)]
pub struct Keyboards {
    pub pressed_keys: HashSet<KeyCode>,
}

impl Keyboards {
    pub fn advance(&mut self, key_event: &KeyEvent) {
        match key_event {
            // NOTE: Ignore the escape key as it is used for main menu navigation
            KeyEvent::Pressed(key) if *key != KeyCode::Escape => {
                self.pressed_keys.insert(*key);
            }
            KeyEvent::Released(key) if *key != KeyCode::Escape => {
                self.pressed_keys.remove(key);
            }
            _ => (),
        };
    }

    pub fn get_joypad(&self, mapping: &JoypadKeyboardMapping) -> JoypadState {
        mapping.calculate_state(&self.pressed_keys)
    }
}
