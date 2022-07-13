pub(crate) mod keyboard;

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

#[derive(Debug)]
pub(crate) struct StaticJoypadInput(pub u8);

impl JoypadInput for StaticJoypadInput {
    fn to_u8(&self) -> u8 {
        self.0
    }
}
