use self::{
    gamepad::{Gamepads, JoypadGamepadMapping},
    keyboard::{JoypadKeyboardMapping, Keyboards},
    keys::{KeyCode, Modifiers},
    sdl2_impl::Sdl2Gamepads,
    settings::InputSettings,
};
use crate::{
    bundle::Bundle,
    settings::{gui::GuiEvent, Settings, MAX_PLAYERS},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fmt::Debug,
    ops::Deref,
    sync::{Arc, RwLock},
};

pub mod buttons;
pub mod gamepad;
pub mod gui;
pub mod keyboard;
pub mod keys;
pub mod sdl2_impl;
pub mod settings;

type GamepadImpl = Sdl2Gamepads;

#[derive(Clone, Debug)]
pub enum KeyEvent {
    Pressed(KeyCode),
    Released(KeyCode),
    ModifiersChanged(Modifiers),
}

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct JoypadMapping<KeyType> {
    pub up: Option<KeyType>,
    pub down: Option<KeyType>,
    pub left: Option<KeyType>,
    pub right: Option<KeyType>,
    pub start: Option<KeyType>,
    pub select: Option<KeyType>,
    pub b: Option<KeyType>,
    pub a: Option<KeyType>,
}

impl<KeyType> JoypadMapping<KeyType>
where
    KeyType: PartialEq + Debug,
{
    pub fn lookup(&mut self, button: &JoypadButton) -> &mut Option<KeyType> {
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

    fn reverse_lookup(&self, key: &KeyType) -> HashSet<JoypadButton> {
        [
            (JoypadButton::Up, &self.up),
            (JoypadButton::Down, &self.down),
            (JoypadButton::Left, &self.left),
            (JoypadButton::Right, &self.right),
            (JoypadButton::Start, &self.start),
            (JoypadButton::Select, &self.select),
            (JoypadButton::B, &self.b),
            (JoypadButton::A, &self.a),
        ]
        .into_iter()
        .fold(HashSet::new(), |mut acc, (joypad_button, mapping)| {
            if let Some(a_key) = mapping {
                if key == a_key {
                    acc.insert(joypad_button);
                }
            }
            acc
        })
    }

    fn calculate_state(&self, keys: &HashSet<KeyType>) -> JoypadState {
        JoypadState(keys.iter().fold(0_u8, |mut acc, key| {
            for button in self.reverse_lookup(key) {
                acc |= button as u8;
            }
            acc
        }))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct JoypadState(pub u8);

impl Deref for JoypadState {
    type Target = u8;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl JoypadState {
    pub fn is_pressed(&self, button: JoypadButton) -> bool {
        self.deref() & (button as u8) != 0
    }
}

pub type InputId = String;
pub trait ToInputId {
    fn to_input_id(&self) -> InputId;
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct InputConfiguration {
    pub id: InputId,
    pub name: String,
    pub kind: InputConfigurationKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub enum InputConfigurationKind {
    Keyboard(JoypadKeyboardMapping),
    Gamepad(JoypadGamepadMapping),
}
#[derive(Debug)]
pub struct MapRequest {
    input_id: InputId,
    button: JoypadButton,
}

pub struct Inputs {
    keyboards: Keyboards,
    gamepads: GamepadImpl,
    pub joypads: Arc<RwLock<[JoypadState; MAX_PLAYERS]>>,
}

impl Inputs {
    pub fn new(gamepads: GamepadImpl) -> Self {
        let keyboards = Keyboards::new();

        Self {
            keyboards,
            gamepads,
            joypads: Arc::new(RwLock::new([JoypadState(0), JoypadState(0)])),
        }
    }

    pub fn advance(&mut self, event: &GuiEvent) {
        match event {
            GuiEvent::Keyboard(key_event) => {
                self.keyboards.advance(key_event);
            }
            GuiEvent::Gamepad(gamepad_event) => {
                self.gamepads.advance(gamepad_event);
            }
        }
        let input_settings = &mut Settings::current_mut().input;
        input_settings.reset_selected_disconnected_inputs(self);

        let pad1 =
            self.get_joypad_for_input_configuration(input_settings.get_selected_configuration(0));
        let pad2 =
            self.get_joypad_for_input_configuration(input_settings.get_selected_configuration(1));
        let mut joypads = self.joypads.write().unwrap();
        joypads[0] = pad1;
        joypads[1] = pad2;
    }

    pub fn get_joypad(&self, player: usize) -> JoypadState {
        self.joypads.read().unwrap()[player]
    }

    pub fn get_default_conf(&self, player: usize) -> &InputConfiguration {
        Bundle::current()
            .config
            .default_settings
            .input
            .get_selected_configuration(player)
    }

    fn get_joypad_for_input_configuration(
        &mut self,
        input_conf: &InputConfiguration,
    ) -> JoypadState {
        match &input_conf.kind {
            InputConfigurationKind::Keyboard(mapping) => self.keyboards.get_joypad(mapping),
            InputConfigurationKind::Gamepad(mapping) => {
                self.gamepads.get_joypad(&input_conf.id, mapping)
            }
        }
    }

    pub fn is_connected(&self, input_conf: &InputConfiguration) -> bool {
        match &input_conf.kind {
            InputConfigurationKind::Keyboard(_) => true,
            InputConfigurationKind::Gamepad(_) => self
                .gamepads
                .get_gamepad_by_input_id(&input_conf.id)
                .map(|gp| gp.is_connected())
                .unwrap_or(false),
        }
    }

    pub fn remap_configuration(
        &mut self,
        mapping_request: &mut Option<MapRequest>,
        input_settings: &mut InputSettings,
    ) {
        let mut remapped = false;
        if let Some(map_request) = mapping_request {
            if let Some(input_configuration) = &mut input_settings
                .configurations
                .get_mut(&map_request.input_id.clone())
            {
                let button = &map_request.button;
                let input_configuration_id = input_configuration.id.clone();
                match &mut input_configuration.kind {
                    InputConfigurationKind::Keyboard(mapping) => {
                        if let Some(code) = self.keyboards.pressed_keys.iter().next() {
                            //If there's any key pressed, use the first found.
                            let _ = mapping.lookup(button).insert(*code);
                            remapped = true;
                        }
                    }
                    InputConfigurationKind::Gamepad(mapping) => {
                        let gamepads = &self.gamepads;
                        if let Some(state) =
                            gamepads.get_gamepad_by_input_id(&input_configuration_id)
                        {
                            if let Some(new_button) = state.get_pressed_buttons().iter().next() {
                                //If there's any button pressed, use the first found.
                                let _ = mapping.lookup(button).insert(*new_button);
                                remapped = true;
                            }
                        }
                    }
                }
            }
        }
        if remapped {
            *mapping_request = None;
        }
    }
}
