use super::{InputId, JoypadInput, JoypadMapping};
use crate::{
    input::{self, InputConfigurationKind},
    settings::input::InputSettings,
};
use sdl2::{controller::GameController, GameControllerSubsystem, Sdl};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

#[derive(Debug, Copy, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
//#[serde(remote = "sdl2::controller::Button")]
pub enum ButtonInt {
    A,
    B,
    X,
    Y,
    Back,
    Guide,
    Start,
    LeftStick,
    RightStick,
    LeftShoulder,
    RightShoulder,
    DPadUp,
    DPadDown,
    DPadRight,
    DPadLeft,
    Misc1,
    Paddle1,
    Paddle2,
    Paddle3,
    Paddle4,
    Touchpad,
}

impl ButtonInt {
    fn map(button: sdl2::controller::Button) -> ButtonInt {
        use sdl2::controller::Button::*;
        match button {
            A => ButtonInt::A,
            B => ButtonInt::B,
            X => ButtonInt::X,
            Y => ButtonInt::Y,
            Back => ButtonInt::Back,
            Guide => ButtonInt::Guide,
            Start => ButtonInt::Start,
            LeftStick => ButtonInt::LeftStick,
            RightStick => ButtonInt::RightStick,
            LeftShoulder => ButtonInt::LeftShoulder,
            RightShoulder => ButtonInt::RightShoulder,
            DPadUp => ButtonInt::DPadUp,
            DPadDown => ButtonInt::DPadDown,
            DPadLeft => ButtonInt::DPadLeft,
            DPadRight => ButtonInt::DPadRight,
            Misc1 => ButtonInt::Misc1,
            Paddle1 => ButtonInt::Paddle1,
            Paddle2 => ButtonInt::Paddle2,
            Paddle3 => ButtonInt::Paddle3,
            Paddle4 => ButtonInt::Paddle4,
            Touchpad => ButtonInt::Touchpad,
        }
    }
}

type Button = ButtonInt;

pub type JoypadGamepadMapping = JoypadMapping<Button>;

pub struct GamepadState {
    pub pressed_buttons: HashSet<Button>,
    game_controller: GameController,
}

impl GamepadState {
    pub fn new(game_controller: GameController) -> Self {
        Self {
            pressed_buttons: HashSet::new(),
            game_controller,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.game_controller.attached()
    }
}
type GamepadId = u32;
type ControllerIdx = u32;

pub struct Gamepads {
    game_controller_subsystem: GameControllerSubsystem,
    all: HashMap<InputId, GamepadState>,
    id_map: HashMap<GamepadId, InputId>,
}

impl Gamepads {
    pub fn new(sdl_context: &Sdl, input_settings: &mut InputSettings) -> Self {
        let game_controller_subsystem = sdl_context.game_controller().unwrap();

        let num_joysticks = game_controller_subsystem.num_joysticks().unwrap();

        let mut res = Gamepads {
            game_controller_subsystem,
            all: HashMap::new(),
            id_map: HashMap::new(),
        };

        //Setup configurations for already connected gamepads
        for idx in 0..num_joysticks {
            res.setup_gamepad_config(idx, input_settings);
        }
        res
    }

    fn map_id(&mut self, gamepad_id: GamepadId) -> &InputId {
        self.id_map
            .entry(gamepad_id)
            .or_insert(format!("01-gamepad-{}", gamepad_id))
    }

    fn get_gamepad(&mut self, gamepad_id: GamepadId) -> Option<&mut GamepadState> {
        let id = self.map_id(gamepad_id).clone();
        self.all.get_mut(&id)
    }

    pub fn get_gamepad_by_input_id(&self, id: &InputId) -> Option<&GamepadState> {
        self.all.get(id)
    }

    pub fn advance(&mut self, input_settings: &mut InputSettings) {
        for event in self
            .game_controller_subsystem
            .sdl()
            .event_pump()
            .unwrap()
            .poll_iter()
        {
            use sdl2::event::Event;
            match event {
                Event::ControllerDeviceAdded { which, .. } => {
                    if let Some(conf) = self.setup_gamepad_config(which, input_settings) {
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
                    } else {
                        eprintln!("Could not setup controller {:?}", which);
                    }
                }
                // Event::ControllerDeviceRemoved { which, .. } => {
                //     self.get_or_create_gamepad(which).disconnected = true;
                // }
                Event::ControllerButtonDown { which, button, .. } => {
                    if let Some(gamepad_state) = self.get_gamepad(which) {
                        gamepad_state.pressed_buttons.insert(ButtonInt::map(button));
                    } else {
                        eprintln!("Button down on unmapped gamepad {:?}", which);
                    }
                }
                Event::ControllerButtonUp { which, button, .. } => {
                    if let Some(gamepad_state) = self.get_gamepad(which) {
                        gamepad_state
                            .pressed_buttons
                            .remove(&ButtonInt::map(button));
                    } else {
                        eprintln!("Button up on unmapped gamepad {:?}", which);
                    }
                }
                _ => (),
            }
        }
    }

    fn setup_gamepad_config(
        &mut self,
        controller_idx: ControllerIdx,
        input_settings: &mut InputSettings,
    ) -> Option<Rc<RefCell<input::InputConfiguration>>> {
        if let Some(found_controller) = (0..self.game_controller_subsystem.num_joysticks().unwrap())
            .find_map(|id| {
                if controller_idx == id && self.game_controller_subsystem.is_game_controller(id) {
                    match self.game_controller_subsystem.open(id) {
                        Ok(c) => Some(c),
                        Err(e) => {
                            eprintln!("Failed to open controller {:?}", e);
                            None
                        }
                    }
                } else {
                    None
                }
            })
        {
            let gamepad_id = found_controller.instance_id();
            let id = self.map_id(gamepad_id).clone();
            self.all
                .insert(id.to_string(), GamepadState::new(found_controller));

            let conf = input_settings.get_or_create_config(
                id.to_string(),
                input::InputConfiguration {
                    name: format!("Gamepad {}", gamepad_id),
                    id,
                    kind: InputConfigurationKind::Gamepad(input_settings.default_gamepad_mapping),
                },
            );
            Some(Rc::clone(conf))
        } else {
            None
        }
    }

    pub fn get_joypad(&mut self, id: &InputId, mapping: &JoypadGamepadMapping) -> JoypadInput {
        if let Some(state) = self.get_gamepad_by_input_id(id) {
            mapping.calculate_state(&state.pressed_buttons)
        } else {
            JoypadInput(0)
        }
    }
}
