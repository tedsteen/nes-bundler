use winit::event::VirtualKeyCode;
use winit::event::VirtualKeyCode::*;
use winit::event::ElementState::*;

pub(crate) struct JoypadMappings {
    pub up: VirtualKeyCode,
    pub down: VirtualKeyCode,
    pub left: VirtualKeyCode,
    pub right: VirtualKeyCode,
    pub a: VirtualKeyCode,
    pub b: VirtualKeyCode,
    pub select: VirtualKeyCode,
    pub start: VirtualKeyCode,
    pub state: u8
}

impl JoypadMappings {
    pub fn apply(&mut self, input: &winit::event::KeyboardInput) -> u8 {
        let code = input.virtual_keycode.unwrap();
        let state = input.state;

        let mask: u8 =
            if code == self.up {
                0b00010000u8
            } else if code == self.down {
                0b00100000u8
            } else if code == self.left {
                0b01000000u8
            } else if code == self.right {
                0b10000000u8
            } else if code == self.start {
                0b00001000u8
            } else if code == self.select {
                0b00000100u8
            } else if code == self.b {
                0b00000010u8
            } else if code == self.a {
                0b00000001u8
            } else {
                0b00000000u8
            };

        if state == Pressed {
            self.state |= mask;
        } else if state == Released {
            self.state ^= mask;
        }

        self.state
    }
    pub const DEFAULT_PAD1: JoypadMappings = JoypadMappings {
        up: Up,
        down: Down,
        left: Left,
        right: Right,
        start: Return,
        select: RShift,
        b: Key1,
        a: Key2,
        state: 0
    };
    
    pub const DEFAULT_PAD2: JoypadMappings = JoypadMappings {
        up: W,
        down: S,
        left: A,
        right: D,
        start: Key9,
        select: Key0,
        b: LAlt,
        a: LControl,
        state: 0
    };
}