use egui_winit::winit as winit;
use winit::event::VirtualKeyCode;
use winit::event::VirtualKeyCode::*;

use crate::settings::MAX_PLAYERS;

use super::{JoypadInput, JoypadKeyMap};

pub(crate) const DEFAULT_KEYBOARD_MAPPINGS: [JoypadKeyMap<VirtualKeyCode>; MAX_PLAYERS] = [
    JoypadKeyMap {
        up: Some(Up), down: Some(Down), left: Some(Left), right: Some(Right),
        start: Some(Return), select: Some(RShift),
        b: Some(Key1), a: Some(Key2),
    },
    JoypadKeyMap {
        up: Some(W), down: Some(S), left: Some(A), right: Some(D),
        start: Some(Key9), select: Some(Key0),
        b: Some(LAlt), a: Some(LControl),
    }
];

pub(crate) struct JoypadKeyboardInput {
    pub(crate) mapping: JoypadKeyMap<VirtualKeyCode>,
    pub(crate) state: u8,
}

impl JoypadKeyboardInput {
    pub(crate) const fn new(mapping: JoypadKeyMap<VirtualKeyCode>) -> Self {
        Self { mapping, state: 0 }
    }
}

impl JoypadInput for JoypadKeyboardInput {
    fn to_u8(&self) -> u8 {
        self.state
    }
}

impl JoypadKeyboardInput {
    pub fn apply(&mut self, input: &winit::event::KeyboardInput) -> u8 {
        let code = input.virtual_keycode.unwrap();
        let buttons = self.mapping.reverse_lookup(&code);
        let mask = buttons
            .iter()
            .fold(0_u8, |acc, &button| acc | button as u8);

        use winit::event::ElementState::*;
        match input.state {
            Pressed => self.state |= mask,
            Released => self.state ^= mask,
        }

        self.state
    }
}