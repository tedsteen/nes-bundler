use super::{InputId, JoypadInput, JoypadKeyMap};
use crate::{
    input::{self, InputConfigurationKind},
    settings::input::InputSettings,
};
use gilrs::{Button, Event, EventType, GamepadId, Gilrs};
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

pub type JoypadGamepadKeyMap = JoypadKeyMap<Button>;

pub struct GamepadState {
    pub pressed_keys: HashSet<Button>,
    pub disconnected: bool,
}

impl GamepadState {
    pub fn new() -> Self {
        Self {
            pressed_keys: HashSet::new(),
            disconnected: false,
        }
    }

    pub fn is_connected(&self) -> bool {
        !self.disconnected
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

    fn get_or_create_gamepad(&mut self, gamepad_id: GamepadId) -> &mut GamepadState {
        let id = self.map_id(gamepad_id).clone();
        self.all.entry(id).or_insert_with(GamepadState::new)
    }

    pub fn get_gamepad_by_input_id(&self, id: &InputId) -> Option<&GamepadState> {
        self.all.get(id)
    }

    pub fn advance(&mut self, input_settings: &mut InputSettings) {
        while let Some(Event {
            id: gamepad_id,
            event,
            ..
        }) = self.gilrs.next_event()
        {
            let id = self.map_id(gamepad_id).clone();

            match event {
                EventType::Connected => {
                    println!("Gamepad connected {}", gamepad_id);
                    self.get_or_create_gamepad(gamepad_id).disconnected = false;
                    let conf = input_settings.get_or_create_config(
                        &id,
                        input::InputConfiguration {
                            name: format!("Gamepad #{}", gamepad_id),
                            id: id.clone(),
                            kind: InputConfigurationKind::Gamepad(
                                Gamepads::create_default_mapping(),
                            ),
                        },
                    );

                    // Automatically select a gamepad if it's connected and keyboard is currently selected.
                    let conf = Rc::clone(conf);
                    if let InputConfigurationKind::Keyboard(_) =
                        Rc::clone(&input_settings.selected[0]).borrow().kind
                    {
                        input_settings.selected[0] = conf;
                    } else if let InputConfigurationKind::Keyboard(_) =
                        Rc::clone(&input_settings.selected[1]).borrow().kind
                    {
                        input_settings.selected[1] = conf;
                    }
                }
                EventType::Disconnected => {
                    println!("Gamepad disconnected {}", gamepad_id);
                    self.get_or_create_gamepad(gamepad_id).disconnected = true;
                }

                EventType::ButtonPressed(button, _) => {
                    self.get_or_create_gamepad(gamepad_id)
                        .pressed_keys
                        .insert(button);
                }
                EventType::ButtonReleased(button, _) => {
                    self.get_or_create_gamepad(gamepad_id)
                        .pressed_keys
                        .remove(&button);
                }

                EventType::ButtonRepeated(_, _) => {}
                EventType::ButtonChanged(_, _, _) => {}
                EventType::AxisChanged(_, _, _) => {}
                EventType::Dropped => {}
            }
            //println!("{:?} New event from {}: {:?}", time, id, event);
        }
    }

    pub fn get_joypad(&mut self, id: &InputId, mapping: &JoypadGamepadKeyMap) -> JoypadInput {
        if let Some(state) = self.get_gamepad_by_input_id(id) {
            mapping.calculate_state(&state.pressed_keys)
        } else {
            JoypadInput(0)
        }
    }
}
