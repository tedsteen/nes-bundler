use self::{
    gamepad::{Gamepads, JoypadGamepadKeyMap},
    keyboard::{JoypadKeyboardKeyMap, Keyboards},
};
use crate::settings::input::{InputConfigurationRef, InputSettings};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, rc::Rc};
use winit::event::Event;

pub mod gamepad;
pub mod keyboard;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JoypadButton {
    Up = 0b00010000,
    Down = 0b00100000,
    Left = 0b01000000,
    Right = 0b10000000,

    Start = 0b00001000,
    Select = 0b00000100,

    B = 0b00000010,
    A = 0b00000001,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Hash, PartialEq)]
pub struct JoypadKeyMap<KeyType> {
    pub up: Option<KeyType>,
    pub down: Option<KeyType>,
    pub left: Option<KeyType>,
    pub right: Option<KeyType>,
    pub start: Option<KeyType>,
    pub select: Option<KeyType>,
    pub b: Option<KeyType>,
    pub a: Option<KeyType>,
}

impl<KeyType> JoypadKeyMap<KeyType>
where
    KeyType: PartialEq,
{
    pub fn lookup_mut(&mut self, button: &JoypadButton) -> &mut Option<KeyType> {
        match button {
            JoypadButton::Up => &mut self.up,
            JoypadButton::Down => &mut self.down,
            JoypadButton::Left => &mut self.left,
            JoypadButton::Right => &mut self.right,
            JoypadButton::Start => &mut self.start,
            JoypadButton::Select => &mut self.select,
            JoypadButton::B => &mut self.b,
            JoypadButton::A => &mut self.a,
        }
    }
    pub fn lookup(&self, button: &JoypadButton) -> &Option<KeyType> {
        match button {
            JoypadButton::Up => &self.up,
            JoypadButton::Down => &self.down,
            JoypadButton::Left => &self.left,
            JoypadButton::Right => &self.right,
            JoypadButton::Start => &self.start,
            JoypadButton::Select => &self.select,
            JoypadButton::B => &self.b,
            JoypadButton::A => &self.a,
        }
    }

    fn insert_if_mapped(
        buttons: &mut HashSet<JoypadButton>,
        mapping: &Option<KeyType>,
        a_key: &KeyType,
        button: JoypadButton,
    ) {
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
        JoypadInput(keys.iter().fold(0_u8, |mut acc, key| {
            for button in self.reverse_lookup(key) {
                acc |= button as u8;
            }
            acc
        }))
    }
}

#[derive(Debug)]
pub struct JoypadInput(pub u8);

impl JoypadInput {
    pub fn is_pressed(&self, button: JoypadButton) -> bool {
        self.0 & (button as u8) != 0
    }
}

pub type InputId = String;

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq)]
pub struct InputConfiguration {
    pub id: InputId,
    pub name: String,
    pub disconnected: bool,
    pub kind: InputConfigurationKind,
}
#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq)]
pub enum InputConfigurationKind {
    Keyboard(JoypadKeyboardKeyMap),
    Gamepad(JoypadGamepadKeyMap),
}
pub struct Inputs {
    keyboards: Keyboards,
    gamepads: Gamepads,
    pub p1: JoypadInput,
    pub p2: JoypadInput,
}

impl Inputs {
    pub fn new() -> Self {
        let gamepads = Gamepads::new();
        let keyboards = Keyboards::new();

        Self {
            keyboards,
            gamepads,
            p1: JoypadInput(0),
            p2: JoypadInput(0),
        }
    }

    pub fn advance(
        &mut self,
        event: &winit::event::Event<()>,
        input_settings: &mut InputSettings,
    ) {
        self.gamepads.advance(input_settings);
        if let Event::WindowEvent {
            event: winit::event::WindowEvent::KeyboardInput { input, .. },
            ..
        } = event
        {
            self.keyboards.advance(input);
        }
        //Check if somethings been disconnected..
        if input_settings.selected[0].borrow().disconnected {
            input_settings.selected[0] = Rc::clone(input_settings.get_default_config(0));
        }
        if input_settings.selected[1].borrow().disconnected {
            input_settings.selected[1] = Rc::clone(input_settings.get_default_config(1));
        }

        self.p1 = self.get_state(&input_settings.selected[0].borrow());
        self.p2 = self.get_state(&input_settings.selected[1].borrow());
    }
    fn get_state(&mut self, input_conf: &InputConfiguration) -> JoypadInput {
        match &input_conf.kind {
            InputConfigurationKind::Keyboard(mapping) => self.keyboards.get(mapping),
            InputConfigurationKind::Gamepad(mapping) => self.gamepads.get(&input_conf.id, mapping),
        }
    }

    pub fn remap_configuration(
        &mut self,
        input_configuration: &InputConfigurationRef,
        button: &JoypadButton,
    ) -> bool {
        match &mut input_configuration.borrow_mut().kind {
            InputConfigurationKind::Keyboard(mapping) => {
                if let Some(code) = self.keyboards.pressed_keys.iter().next() {
                    //If there's any key pressed, use the first found.
                    let _ = mapping.lookup_mut(button).insert(*code);
                    return true;
                }
            }
            InputConfigurationKind::Gamepad(mapping) => {
                if let Some(code) = self
                    .gamepads
                    .get_gamepad_by_input_id(&input_configuration.borrow().id)
                    .pressed_keys
                    .iter()
                    .next()
                {
                    //If there's any button pressed, use the first found.
                    let _ = mapping.lookup_mut(button).insert(*code);
                    return true;
                }
            }
        }
        false
    }
}
