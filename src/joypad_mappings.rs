use winit::event::VirtualKeyCode;
use winit_input_helper::WinitInputHelper;

pub(crate) struct JoypadMappings {
    pub up: VirtualKeyCode,
    pub down: VirtualKeyCode,
    pub left: VirtualKeyCode,
    pub right: VirtualKeyCode,
    pub a: VirtualKeyCode,
    pub b: VirtualKeyCode,
    pub select: VirtualKeyCode,
    pub start: VirtualKeyCode
}

impl JoypadMappings {
    pub fn to_pad(&mut self, input: &WinitInputHelper) -> u8 {
        let mut pad_data: u8 = 0;
        if input.key_held(self.up) {
            pad_data |= 0b00010000u8;
        }
        if input.key_held(self.down) {
            pad_data |= 0b00100000u8;
        }
        if input.key_held(self.left) {
            pad_data |= 0b01000000u8;
        }
        if input.key_held(self.right) {
            pad_data |= 0b10000000u8;
        }

        if input.key_held(self.start) {
            pad_data |= 0b10001000u8;
        }
        if input.key_held(self.select) {
            pad_data |= 0b00000100u8;
        }

        if input.key_held(self.b) {
            pad_data |= 0b00000010u8;
        }
        if input.key_held(self.a) {
            pad_data |= 0b00000001u8;
        }

        pad_data
    }
}