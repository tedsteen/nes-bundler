use std::collections::HashSet;

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

#[derive(Clone, Copy)]
pub(crate) struct JoypadKeyMap<KeyType> {
    up: Option<KeyType>,
    down: Option<KeyType>,
    left: Option<KeyType>,
    right: Option<KeyType>,
    start: Option<KeyType>,
    select: Option<KeyType>,
    b: Option<KeyType>,
    a: Option<KeyType>
}

impl<KeyType> JoypadKeyMap<KeyType> where
    KeyType: PartialEq
{
    pub(crate) fn lookup(&mut self, button: &JoypadButton) -> &mut Option<KeyType> {
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
    fn insert_if_mapped(buttons: &mut HashSet<JoypadButton>, mapping: &Option<KeyType>, key_code: &KeyType, button: JoypadButton) {
        if let Some(key) = mapping {
            if key.eq(key_code) {
                buttons.insert(button);
            }
        }
    }
    fn reverse_lookup(&self, key_code: &KeyType) -> HashSet<JoypadButton> {
        let mut buttons = HashSet::new();
        JoypadKeyMap::insert_if_mapped(&mut buttons, &self.up, key_code, JoypadButton::Up);
        JoypadKeyMap::insert_if_mapped(&mut buttons, &self.down, key_code, JoypadButton::Down);
        JoypadKeyMap::insert_if_mapped(&mut buttons, &self.left, key_code, JoypadButton::Left);
        JoypadKeyMap::insert_if_mapped(&mut buttons, &self.right, key_code, JoypadButton::Right);
        JoypadKeyMap::insert_if_mapped(&mut buttons, &self.start, key_code, JoypadButton::Start);
        JoypadKeyMap::insert_if_mapped(&mut buttons, &self.select, key_code, JoypadButton::Select);
        JoypadKeyMap::insert_if_mapped(&mut buttons, &self.b, key_code, JoypadButton::B);
        JoypadKeyMap::insert_if_mapped(&mut buttons, &self.a, key_code, JoypadButton::A);
        buttons
    }
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
