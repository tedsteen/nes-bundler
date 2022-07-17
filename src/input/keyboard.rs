use std::collections::HashSet;
use egui_winit::winit as winit;
use winit::event::{VirtualKeyCode, KeyboardInput};
use super::{JoypadKeyMap, JoypadInput};

pub(crate) type JoypadKeyboardKeyMap = JoypadKeyMap<VirtualKeyCode>;

pub(crate) struct Keyboards {
    pub(crate) pressed_keys: HashSet<VirtualKeyCode>
}
impl Keyboards {
    pub(crate) fn new() -> Self {
        Keyboards { pressed_keys: HashSet::new() }
    }
    pub(crate) fn advance(&mut self, input: &KeyboardInput) {
        let key = input.virtual_keycode.unwrap();
        match input.state {
            winit::event::ElementState::Pressed => self.pressed_keys.insert(key),
            winit::event::ElementState::Released => self.pressed_keys.remove(&key),
        };
    }

    pub(crate) fn get(&mut self, mapping: &JoypadKeyboardKeyMap) -> JoypadInput {
        mapping.calculate_state(&self.pressed_keys)
    }
}