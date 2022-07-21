use super::{InputId, JoypadInput, JoypadKeyMap};
use crate::{
    input::{self, InputConfigurationKind},
    settings::input::InputSettings,
};
use gilrs::{Button, Event, EventType, GamepadId, Gilrs};
use std::collections::{HashMap, HashSet};

pub type JoypadGamepadKeyMap = JoypadKeyMap<Button>;

pub struct GamepadState {
    pub pressed_keys: HashSet<Button>,
}

impl GamepadState {
    pub fn new() -> Self {
        Self {
            pressed_keys: HashSet::new(),
        }
    }
}

pub struct Gamepads {
    gilrs: Gilrs,
    all: HashMap<InputId, GamepadState>,
    id_map: HashMap<GamepadId, InputId>,
}

impl Gamepads {
    pub fn new() -> Self {
        Gamepads {
            gilrs: Gilrs::new().unwrap(),
            all: HashMap::new(),
            id_map: HashMap::new(),
        }
    }
    fn create_default_mapping() -> JoypadGamepadKeyMap {
        JoypadGamepadKeyMap {
            up: Some(Button::DPadUp),
            down: Some(Button::DPadDown),
            left: Some(Button::DPadLeft),
            right: Some(Button::DPadRight),
            start: Some(Button::Start),
            select: Some(Button::Select),
            b: Some(Button::West),
            a: Some(Button::South),
        }
    }
    fn map_id(&mut self, gamepad_id: GamepadId) -> &InputId {
        self.id_map
            .entry(gamepad_id)
            .or_insert(format!("01-gamepad-{}", gamepad_id))
    }

    fn get_gamepad(&mut self, gamepad_id: GamepadId) -> &mut GamepadState {
        let id = self.map_id(gamepad_id).clone();
        self.all.entry(id).or_insert_with(GamepadState::new)
    }

    pub fn get_gamepad_by_input_id(&mut self, id: &InputId) -> &mut GamepadState {
        self.all.entry(id.clone()).or_insert_with(GamepadState::new)
    }

    pub fn advance(&mut self, input_settings: &mut InputSettings) {
        while let Some(Event {
            id: gamepad_id,
            event,
            ..
        }) = self.gilrs.next_event()
        {
            let id = self.map_id(gamepad_id);

            match event {
                EventType::Connected => {
                    println!("Gamepad connected {}", gamepad_id);
                    let mut conf = input_settings
                        .get_or_create_config(
                            id,
                            input::InputConfiguration {
                                name: format!("Gamepad #{}", gamepad_id),
                                id: id.clone(),
                                disconnected: false,
                                kind: InputConfigurationKind::Gamepad(
                                    Gamepads::create_default_mapping(),
                                ),
                            },
                        )
                        .borrow_mut();
                    conf.disconnected = false;
                }
                EventType::Disconnected => {
                    println!("Gamepad disconnected {}", gamepad_id);
                    let mut conf = input_settings
                        .get_or_create_config(
                            id,
                            input::InputConfiguration {
                                name: format!("Gamepad #{}", gamepad_id),
                                id: id.clone(),
                                disconnected: false,
                                kind: InputConfigurationKind::Gamepad(
                                    Gamepads::create_default_mapping(),
                                ),
                            },
                        )
                        .borrow_mut();
                    conf.disconnected = true;
                }

                EventType::ButtonPressed(button, _) => {
                    self.get_gamepad(gamepad_id).pressed_keys.insert(button);
                }
                EventType::ButtonReleased(button, _) => {
                    self.get_gamepad(gamepad_id).pressed_keys.remove(&button);
                }

                EventType::ButtonRepeated(_, _) => {}
                EventType::ButtonChanged(_, _, _) => {}
                EventType::AxisChanged(_, _, _) => {}
                EventType::Dropped => {}
            }
            //println!("{:?} New event from {}: {:?}", time, id, event);
        }
    }

    pub fn get(&mut self, id: &InputId, mapping: &JoypadGamepadKeyMap) -> JoypadInput {
        mapping.calculate_state(&self.get_gamepad_by_input_id(id).pressed_keys)
    }
}
