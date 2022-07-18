use std::{collections::HashSet};

use winit::event::Event;

use crate::settings::{InputSettings};

use self::{keyboard::{Keyboards, JoypadKeyboardKeyMap}, gamepad::{Gamepads, JoypadGamepadKeyMap}};

pub(crate) mod keyboard;
pub(crate) mod gamepad;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum JoypadButton {
    Up = 0b00010000,
    Down = 0b00100000,
    Left = 0b01000000,
    Right = 0b10000000,

    Start = 0b00001000,
    Select = 0b00000100,

    B = 0b00000010,
    A = 0b00000001,
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

    fn calculate_state(&self, keys: &HashSet<KeyType>) -> JoypadInput {
        JoypadInput(keys
            .iter()
            .fold(0_u8, |mut acc, key| {
                for button in self.reverse_lookup(key) {
                    acc |= button as u8;
                }
                acc
            }))
    }
}

#[derive(Debug)]
pub(crate) struct JoypadInput(pub(crate) u8);

impl JoypadInput {
    pub(crate) fn is_pressed(&self, button: JoypadButton) -> bool {
        self.0 & (button as u8) != 0
    }
}

pub(crate) type InputId = String;

#[derive(Debug, Clone)]
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
    keyboards: Keyboards,
    gamepads: Gamepads,
    pub(crate) p1: JoypadInput,
    pub(crate) p2: JoypadInput
}

impl Inputs {
    pub(crate) fn new() -> Self {
        let gamepads = Gamepads::new();
        let keyboards = Keyboards::new();

        Self { keyboards, gamepads, p1: JoypadInput(0), p2: JoypadInput(0) }
    }
    
    pub(crate) fn advance(&mut self, event: &winit::event::Event<()>, input_settings: &mut InputSettings) {
        self.gamepads.advance(input_settings);
        if let Event::WindowEvent { event: winit::event::WindowEvent::KeyboardInput { input, .. }, .. } = event {
            self.keyboards.advance(input);
        }

        self.p1 = self.get_state(input_settings.get_config(0));
        self.p2 = self.get_state(input_settings.get_config(1));
    }
    fn get_state(&mut self, input_conf: &mut InputConfiguration) -> JoypadInput {
        match &input_conf.kind {
            InputConfigurationKind::Keyboard(mapping) => {
                self.keyboards.get(mapping)
            },
            InputConfigurationKind::Gamepad(mapping) => {
                self.gamepads.get(&input_conf.id, mapping)
            },
        }
    }

    pub(crate) fn remap_configuration(&mut self, input_configuration: &mut InputConfiguration, button: &JoypadButton) -> bool {
        match &mut input_configuration.kind {
            InputConfigurationKind::Keyboard(mapping) => {
                if let Some(code) = self.keyboards.pressed_keys.iter().next() {
                    //If there's any key pressed, use the first found.
                    let _ = mapping.lookup(button).insert(*code);
                    return true;
                }
            },
            InputConfigurationKind::Gamepad(mapping) => {
                if let Some(code) = self.gamepads.get_gamepad_by_input_id(&input_configuration.id).pressed_keys.iter().next() {
                    //If there's any button pressed, use the first found.
                    let _ = mapping.lookup(button).insert(*code);
                    return true;
                }
            }
        }
        false
    }
}