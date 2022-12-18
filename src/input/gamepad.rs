use super::{InputId, JoypadInput, JoypadMapping};
use crate::{
    input::{self, InputConfigurationKind},
    settings::input::InputSettings,
};
use gilrs::{Button, Event, EventType, GamepadId, Gilrs};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

pub type JoypadGamepadMapping = JoypadMapping<Button>;

pub struct GamepadState {
    pub pressed_buttons: HashSet<Button>,
    pub disconnected: bool,
}

impl GamepadState {
    pub fn new() -> Self {
        Self {
            pressed_buttons: HashSet::new(),
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
    pub fn new(input_settings: &mut InputSettings) -> Self {
        let gilrs = Gilrs::new().unwrap();
        let available_gamepads = gilrs
            .gamepads()
            .map(|(id, _)| id)
            .collect::<HashSet<GamepadId>>();

        let mut res = Gamepads {
            gilrs,
            all: HashMap::new(),
            id_map: HashMap::new(),
        };

        //Setup configurations for already connected gamepads
        for gamepad_id in available_gamepads {
            res.setup_gamepad_config(gamepad_id, input_settings);
        }
        res
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
            match event {
                EventType::Connected => {
                    let conf = self.setup_gamepad_config(gamepad_id, input_settings);

                    // Automatically select a gamepad if it's connected and keyboard is currently selected.
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
                    self.get_or_create_gamepad(gamepad_id).disconnected = true;
                }

                EventType::ButtonPressed(button, _) => {
                    self.get_or_create_gamepad(gamepad_id)
                        .pressed_buttons
                        .insert(button);
                }
                EventType::ButtonReleased(button, _) => {
                    self.get_or_create_gamepad(gamepad_id)
                        .pressed_buttons
                        .remove(&button);
                }
                _ => {}
            }
            //println!("{:?} New event from {}: {:?}", time, id, event);
        }
    }

    fn setup_gamepad_config(
        &mut self,
        gamepad_id: GamepadId,
        input_settings: &mut InputSettings,
    ) -> Rc<RefCell<input::InputConfiguration>> {
        self.get_or_create_gamepad(gamepad_id).disconnected = false;
        let id = self.map_id(gamepad_id);
        let conf = input_settings.get_or_create_config(
            id,
            input::InputConfiguration {
                name: format!("Gamepad {}", gamepad_id),
                id: id.clone(),
                kind: InputConfigurationKind::Gamepad(input_settings.default_gamepad_mapping),
            },
        );
        Rc::clone(conf)
    }

    pub fn get_joypad(&mut self, id: &InputId, mapping: &JoypadGamepadMapping) -> JoypadInput {
        if let Some(state) = self.get_gamepad_by_input_id(id) {
            mapping.calculate_state(&state.pressed_buttons)
        } else {
            JoypadInput(0)
        }
    }
}
