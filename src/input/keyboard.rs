use std::collections::HashSet;
use egui_winit::winit as winit;
use winit::event::{VirtualKeyCode, KeyboardInput};
use super::{JoypadKeyMap, JoypadInput, InputConfiguration, InputConfigurationKind};

pub(crate) type JoypadKeyboardKeyMap = JoypadKeyMap<VirtualKeyCode>;

pub(crate) struct Keyboards {
    pub(crate) pressed_keys: HashSet<VirtualKeyCode>
}
use winit::event::VirtualKeyCode::*;
impl Keyboards {
    
    pub(crate) fn default_configurations(player: usize) -> InputConfiguration {
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
        ][player].clone()
    }

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