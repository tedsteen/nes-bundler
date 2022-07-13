use std::collections::{HashSet};
use egui_winit::winit as winit;
use winit::event::VirtualKeyCode;
use winit::event::VirtualKeyCode::*;

use super::{JoypadButton, JoypadInput};

pub(crate) struct JoypadKeyMap {
    up: Option<VirtualKeyCode>,
    down: Option<VirtualKeyCode>,
    left: Option<VirtualKeyCode>,
    right: Option<VirtualKeyCode>,
    start: Option<VirtualKeyCode>,
    select: Option<VirtualKeyCode>,
    b: Option<VirtualKeyCode>,
    a: Option<VirtualKeyCode>
}

impl JoypadKeyMap {
    pub(crate) fn lookup(&mut self, button: &JoypadButton) -> &mut Option<VirtualKeyCode> {
        match button {
            JoypadButton::Up => &mut self.up,
            JoypadButton::Down => &mut self.down,
            JoypadButton::Left => &mut self.left,
            JoypadButton::Right => &mut self.right,
            JoypadButton::Start => &mut self.start,
            JoypadButton::Select => &mut self.select,
            JoypadButton::B => &mut self.b,
            JoypadButton::A => &mut self.a
        }
    }
    fn insert_if_mapped(buttons: &mut HashSet<JoypadButton>, mapping: Option<VirtualKeyCode>, key_code: &VirtualKeyCode, button: JoypadButton) {
        if let Some(key) = mapping {
            if key.eq(key_code) {
                buttons.insert(button);
            }
        }
    }
    fn reverse_lookup(&self, key_code: &VirtualKeyCode) -> HashSet<JoypadButton> {
        let mut buttons = HashSet::new();
        JoypadKeyMap::insert_if_mapped(&mut buttons, self.up, key_code, JoypadButton::Up);
        JoypadKeyMap::insert_if_mapped(&mut buttons, self.down, key_code, JoypadButton::Down);
        JoypadKeyMap::insert_if_mapped(&mut buttons, self.left, key_code, JoypadButton::Left);
        JoypadKeyMap::insert_if_mapped(&mut buttons, self.right, key_code, JoypadButton::Right);
        JoypadKeyMap::insert_if_mapped(&mut buttons, self.start, key_code, JoypadButton::Start);
        JoypadKeyMap::insert_if_mapped(&mut buttons, self.select, key_code, JoypadButton::Select);
        JoypadKeyMap::insert_if_mapped(&mut buttons, self.b, key_code, JoypadButton::B);
        JoypadKeyMap::insert_if_mapped(&mut buttons, self.a, key_code, JoypadButton::A);
        buttons
    }

    pub(crate) const fn default_pad1() -> JoypadKeyMap {
        JoypadKeyMap {
            up: Some(Up), down: Some(Down), left: Some(Left), right: Some(Right),
            start: Some(Return), select: Some(RShift),
            b: Some(Key1), a: Some(Key2),
        }
    }
    pub(crate) const fn default_pad2() -> JoypadKeyMap {
        JoypadKeyMap {
            up: Some(W), down: Some(S), left: Some(A), right: Some(D),
            start: Some(Key9), select: Some(Key0),
            b: Some(LAlt), a: Some(LControl),
        }
    }
}

pub(crate) struct JoypadKeyboardInput {
    pub(crate) mapping: JoypadKeyMap,
    pub(crate) state: u8,
}

impl JoypadKeyboardInput {
    pub(crate) const fn new(mapping: JoypadKeyMap) -> Self {
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