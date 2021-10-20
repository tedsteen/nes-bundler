use std::collections::{ HashMap, HashSet };

use winit::event::VirtualKeyCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum JoypadButton {
    UP = 0b00010000isize,
    DOWN = 0b00100000isize,
    LEFT = 0b01000000isize,
    RIGHT = 0b10000000isize,

    START = 0b00001000isize,
    SELECT = 0b00000100isize,

    B = 0b00000010isize,
    A = 0b00000001isize
}

pub(crate) trait JoypadInput {
    fn is_pressed(self: &Self, button: JoypadButton) -> bool {
        self.to_u8() & (button as u8) != 0
    }

    fn to_u8(self: &Self) -> u8;
}
pub(crate) struct JoypadKeyMap {
    map: HashMap<JoypadButton, VirtualKeyCode>,
}

use winit::event::VirtualKeyCode::*;
impl JoypadKeyMap {
    fn new(up: VirtualKeyCode, down: VirtualKeyCode, left: VirtualKeyCode, right: VirtualKeyCode, start: VirtualKeyCode, select: VirtualKeyCode, b: VirtualKeyCode, a: VirtualKeyCode) -> Self {
        let mut map = HashMap::new();
        map.insert(JoypadButton::UP, up);
        map.insert(JoypadButton::DOWN, down);
        map.insert(JoypadButton::LEFT, left);
        map.insert(JoypadButton::RIGHT, right);
        map.insert(JoypadButton::START, start);
        map.insert(JoypadButton::SELECT, select);
        map.insert(JoypadButton::B, b);
        map.insert(JoypadButton::A, a);
        Self { map }
    }

    pub(crate) fn lookup(self: &mut Self, button: &JoypadButton) -> &mut VirtualKeyCode {
        self.map.get_mut(button).unwrap()
    }
    
    fn reverse_lookup(self: &Self, key_code: &VirtualKeyCode) -> HashSet<JoypadButton> {
        let mut buttons = HashSet::new();

        for (button, key) in self.map.clone() {
            if key.eq(key_code) {
                buttons.insert(button);
            }
        }
        buttons
    }
    
    pub(crate) fn default_pad1() -> JoypadKeyMap {
        JoypadKeyMap::new(Up, Down, Left, Right, Return, RShift, Key1, Key2)
    }
    pub(crate) fn default_pad2() -> JoypadKeyMap {
        JoypadKeyMap::new(W, S, A, D, Key9, Key0, LAlt, LControl)
    }
}

pub(crate) struct JoypadKeyboardInput {
    pub(crate) mapping: JoypadKeyMap,
    pub(crate) state: u8
}

impl JoypadKeyboardInput {
    pub(crate) fn new(mapping: JoypadKeyMap) -> Self {
        Self { mapping, state: 0 }
    }
}

impl JoypadInput for JoypadKeyboardInput {
    fn to_u8(self: &Self) -> u8 {
        self.state
    }
}

impl JoypadKeyboardInput {
    pub fn apply(&mut self, input: &winit::event::KeyboardInput) -> u8 {
        let code = input.virtual_keycode.unwrap();
        let mapping = &self.mapping;
        let buttons = mapping.reverse_lookup(&code);

        let mask = buttons.iter().fold(0 as u8, |acc, button| acc | *button as u8 );
        
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
    fn to_u8(self: &Self) -> u8 {
        self.0
    }
}
