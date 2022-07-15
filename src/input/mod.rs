use std::collections::HashSet;

use winit::event::KeyboardInput;

use crate::settings::{Settings};

use self::{keyboard::{Keyboards, JoypadKeyboardKeyMap}, gamepad::{Gamepads, JoypadGamepadKeyMap}};

pub(crate) mod keyboard;
pub(crate) mod gamepad;

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

#[derive(Debug, Clone, Copy)]
pub(crate) struct JoypadKeyMap<KeyType> {
    pub(crate) up: Option<KeyType>,
    pub(crate) down: Option<KeyType>,
    pub(crate) left: Option<KeyType>,
    pub(crate) right: Option<KeyType>,
    pub(crate) start: Option<KeyType>,
    pub(crate) select: Option<KeyType>,
    pub(crate) b: Option<KeyType>,
    pub(crate) a: Option<KeyType>
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
    fn insert_if_mapped(buttons: &mut HashSet<JoypadButton>, mapping: &Option<KeyType>, a_key: &KeyType, button: JoypadButton) {
        if let Some(key) = mapping {
            if a_key.eq(key) {
                buttons.insert(button);
            }
        }
    }
    fn reverse_lookup(&self, key: &KeyType) -> HashSet<JoypadButton> {
        let mut buttons = HashSet::new();
        JoypadKeyMap::insert_if_mapped(&mut buttons, &self.up, key, JoypadButton::Up);
        JoypadKeyMap::insert_if_mapped(&mut buttons, &self.down, key, JoypadButton::Down);
        JoypadKeyMap::insert_if_mapped(&mut buttons, &self.left, key, JoypadButton::Left);
        JoypadKeyMap::insert_if_mapped(&mut buttons, &self.right, key, JoypadButton::Right);
        JoypadKeyMap::insert_if_mapped(&mut buttons, &self.start, key, JoypadButton::Start);
        JoypadKeyMap::insert_if_mapped(&mut buttons, &self.select, key, JoypadButton::Select);
        JoypadKeyMap::insert_if_mapped(&mut buttons, &self.b, key, JoypadButton::B);
        JoypadKeyMap::insert_if_mapped(&mut buttons, &self.a, key, JoypadButton::A);
        buttons
    }

    fn calculate_state(&self, keys: &HashSet<KeyType>) -> StaticJoypadInput {
        let mut buttons = HashSet::new();
        for key in keys {
            buttons.extend(self.reverse_lookup(key));
        }
        let state = buttons
            .iter()
            .fold(0_u8, |acc, &button| acc | button as u8);
        StaticJoypadInput(state)
    }
}

pub(crate) trait JoypadInput {
    fn is_pressed(&self, button: JoypadButton) -> bool {
        self.to_u8() & (button as u8) != 0
    }
    
    fn to_mask(buttons: HashSet<JoypadButton>) -> u8 {
        buttons
            .iter()
            .fold(0_u8, |acc, &button| acc | button as u8)
    }

    fn to_u8(&self) -> u8;

    fn get_name(&self) -> String;
}

#[derive(Debug)]
pub(crate) struct StaticJoypadInput(pub u8);

impl JoypadInput for StaticJoypadInput {
    fn to_u8(&self) -> u8 {
        self.0
    }

    fn get_name(&self) -> String {
        format!("Static [{}]", self.0)
    }
}

pub(crate) type InputId = String;

#[derive(Debug)]
pub(crate) struct InputConfiguration {
    pub(crate) id: InputId,
    pub(crate) name: String,
    pub(crate) disconnected: bool,
    pub(crate) kind: InputConfigurationKind
}
#[derive(Debug, Clone)]
pub(crate) enum InputConfigurationKind {
    Keyboard(JoypadKeyboardKeyMap),
    Gamepad(JoypadGamepadKeyMap)
}
pub(crate) struct Inputs {
    pub(crate) keyboards: Keyboards,
    pub(crate) gamepads: Gamepads,
    pub(crate) p1: StaticJoypadInput,
    pub(crate) p2: StaticJoypadInput
}

impl Inputs {
    pub(crate) fn new() -> Self {
        let gamepads = Gamepads::new();
        let keyboards = Keyboards::new();

        Self { keyboards, gamepads, p1: StaticJoypadInput(0), p2: StaticJoypadInput(0) }
    }
    
    pub(crate) fn advance(&mut self, input: Option<&KeyboardInput>, settings: &mut Settings) {
        self.gamepads.advance(settings);
        if let Some(input) = input {
            self.keyboards.advance(input);
        }

        self.p1 = self.advance_input(settings.get_p1_config());
        self.p2 = self.advance_input(settings.get_p2_config());
    }
    fn advance_input(&mut self, input_conf: &mut InputConfiguration) -> StaticJoypadInput {
        match &input_conf.kind {
            InputConfigurationKind::Keyboard(mapping) => {
                self.keyboards.get(mapping)
            },
            InputConfigurationKind::Gamepad(mapping) => {
                self.gamepads.get(&input_conf.id, mapping)
            },
        }
    }
}