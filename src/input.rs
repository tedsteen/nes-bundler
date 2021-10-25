use std::collections::{HashMap, HashSet};

use winit::event::VirtualKeyCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum JoypadButton {
    Up = 0b00010000isize,
    Down = 0b00100000isize,
    Left = 0b01000000isize,
    Right = 0b10000000isize,

    Start = 0b00001000isize,
    Select = 0b00000100isize,

    B = 0b00000010isize,
    A = 0b00000001isize,
}

pub(crate) trait JoypadInput {
    fn is_pressed(&self, button: JoypadButton) -> bool {
        self.to_u8() & (button as u8) != 0
    }

    fn to_u8(&self) -> u8;
}
pub(crate) struct JoypadKeyMap {
    map: HashMap<JoypadButton, Option<VirtualKeyCode>>,
}

use winit::event::VirtualKeyCode::*;
impl JoypadKeyMap {
    #[allow(clippy::too_many_arguments)]
    fn new(
        up: Option<VirtualKeyCode>,
        down: Option<VirtualKeyCode>,
        left: Option<VirtualKeyCode>,
        right: Option<VirtualKeyCode>,
        start: Option<VirtualKeyCode>,
        select: Option<VirtualKeyCode>,
        b: Option<VirtualKeyCode>,
        a: Option<VirtualKeyCode>,
    ) -> Self {
        let mut map = HashMap::new();
        use JoypadButton::*;
        map.insert(Up, up);
        map.insert(Down, down);
        map.insert(Left, left);
        map.insert(Right, right);
        map.insert(Start, start);
        map.insert(Select, select);
        map.insert(B, b);
        map.insert(A, a);
        Self { map }
    }

    pub(crate) fn lookup(&mut self, button: &JoypadButton) -> &mut Option<VirtualKeyCode> {
        self.map.get_mut(button).unwrap()
    }

    fn reverse_lookup(&self, key_code: &VirtualKeyCode) -> HashSet<&JoypadButton> {
        let mut buttons = HashSet::new();

        for (button, key) in &self.map {
            if let Some(key) = key {
                if key.eq(key_code) {
                    buttons.insert(button);
                }
            }
        }
        buttons
    }

    pub(crate) fn default_pad1() -> JoypadKeyMap {
        JoypadKeyMap::new(
            Some(Up),
            Some(Down),
            Some(Left),
            Some(Right),
            Some(Return),
            Some(RShift),
            Some(Key1),
            Some(Key2),
        )
    }
    pub(crate) fn default_pad2() -> JoypadKeyMap {
        JoypadKeyMap::new(
            Some(W),
            Some(S),
            Some(A),
            Some(D),
            Some(Key9),
            Some(Key0),
            Some(LAlt),
            Some(LControl),
        )
    }
    pub(crate) fn unmapped() -> JoypadKeyMap {
        JoypadKeyMap::new(None, None, None, None, None, None, None, None)
    }
}

pub(crate) struct JoypadKeyboardInput {
    pub(crate) mapping: JoypadKeyMap,
    pub(crate) state: u8,
}

impl JoypadKeyboardInput {
    pub(crate) fn new(mapping: JoypadKeyMap) -> Self {
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
            .fold(0_u8, |acc, &button| acc | *button as u8);

        use winit::event::ElementState::*;
        match input.state {
            Pressed => self.state |= mask,
            Released => self.state ^= mask,
        }

        self.state
    }
}

#[derive(Debug)]
pub(crate) struct StaticJoypadInput(pub u8);

impl JoypadInput for StaticJoypadInput {
    fn to_u8(&self) -> u8 {
        self.0
    }
}
