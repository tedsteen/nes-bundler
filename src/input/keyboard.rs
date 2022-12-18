use super::{JoypadInput, JoypadMapping};
use std::collections::HashSet;
use winit::event::{KeyboardInput, VirtualKeyCode};

pub type JoypadKeyboardMapping = JoypadMapping<VirtualKeyCode>;

pub struct Keyboards {
    pub pressed_keys: HashSet<VirtualKeyCode>,
}

impl Keyboards {
    pub fn new() -> Self {
        Keyboards {
            pressed_keys: HashSet::new(),
        }
    }
    pub fn advance(&mut self, input: &KeyboardInput) {
        if let Some(key) = input.virtual_keycode {
            match input.state {
                winit::event::ElementState::Pressed => self.pressed_keys.insert(key),
                winit::event::ElementState::Released => self.pressed_keys.remove(&key),
            };
        }
    }

    pub fn get_joypad(&mut self, mapping: &JoypadKeyboardMapping) -> JoypadInput {
        mapping.calculate_state(&self.pressed_keys)
    }
}
