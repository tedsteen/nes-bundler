use super::{InputConfiguration, InputConfigurationKind, JoypadInput, JoypadKeyMap};
use egui_winit::winit;
use std::collections::HashSet;
use winit::event::{KeyboardInput, VirtualKeyCode};

pub type JoypadKeyboardKeyMap = JoypadKeyMap<VirtualKeyCode>;

pub struct Keyboards {
    pub pressed_keys: HashSet<VirtualKeyCode>,
}
use winit::event::VirtualKeyCode::*;
impl Keyboards {
    pub fn default_configurations(player: usize) -> InputConfiguration {
        [
            InputConfiguration {
                name: "Keyboard mapping #1".to_string(),
                id: "00-keyboard-1".to_string(),
                kind: InputConfigurationKind::Keyboard(JoypadKeyboardKeyMap {
                    up: Some(Up),
                    down: Some(Down),
                    left: Some(Left),
                    right: Some(Right),
                    start: Some(Return),
                    select: Some(RShift),
                    b: Some(Key1),
                    a: Some(Key2),
                }),
            },
            InputConfiguration {
                name: "Keyboard mapping #2".to_string(),
                id: "00-keyboard-2".to_string(),
                kind: InputConfigurationKind::Keyboard(JoypadKeyboardKeyMap {
                    up: Some(W),
                    down: Some(S),
                    left: Some(A),
                    right: Some(D),
                    start: Some(Key9),
                    select: Some(Key0),
                    b: Some(LAlt),
                    a: Some(LControl),
                }),
            },
        ][player]
            .clone()
    }

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

    pub fn get_joypad(&mut self, mapping: &JoypadKeyboardKeyMap) -> JoypadInput {
        mapping.calculate_state(&self.pressed_keys)
    }
}
