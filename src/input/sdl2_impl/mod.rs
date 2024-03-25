use super::buttons::ToGamepadButton;
use super::{buttons::GamepadButton, InputId, JoypadState};
use super::{InputConfiguration, ToInputId};
use crate::input::{self, InputConfigurationKind};
use crate::settings::Settings;
use std::collections::{HashMap, HashSet};

use sdl2::{controller::GameController, GameControllerSubsystem};

use super::gamepad::{GamepadEvent, GamepadState, Gamepads, JoypadGamepadMapping, ToGamepadEvent};

pub struct Sdl2GamepadState {
    pub pressed_buttons: HashSet<GamepadButton>,
    game_controller: GameController,
}

impl Sdl2GamepadState {
    pub fn new(game_controller: GameController) -> Self {
        Self {
            pressed_buttons: HashSet::new(),
            game_controller,
        }
    }
}

impl ToInputId for u32 {
    fn to_input_id(&self) -> InputId {
        self.to_string()
    }
}

impl GamepadState for Sdl2GamepadState {
    fn is_connected(&self) -> bool {
        self.game_controller.attached()
    }

    fn get_pressed_buttons(&self) -> &HashSet<GamepadButton> {
        &self.pressed_buttons
    }

    fn toogle_button(&mut self, button: &GamepadButton, pressed: bool) {
        if pressed {
            self.pressed_buttons.insert(*button);
        } else {
            self.pressed_buttons.remove(button);
        }
    }
}
pub struct Sdl2Gamepads {
    game_controller_subsystem: GameControllerSubsystem,
    all: HashMap<InputId, Box<dyn GamepadState>>,
}

impl Gamepads for Sdl2Gamepads {
    fn get_joypad(&mut self, id: &InputId, mapping: &JoypadGamepadMapping) -> JoypadState {
        if let Some(state) = self.get_gamepad_by_input_id(id) {
            mapping.calculate_state(state.get_pressed_buttons())
        } else {
            JoypadState(0)
        }
    }

    fn get_gamepad_by_input_id(&self, id: &InputId) -> Option<&dyn GamepadState> {
        self.all.get(id).map(|a| a.as_ref())
    }

    fn advance(&mut self, gamepad_event: &GamepadEvent) {
        let input_settings = &mut Settings::current().input;
        match gamepad_event {
            GamepadEvent::ControllerAdded { which, .. } => {
                if let Some(conf) = self.setup_gamepad_config(which.clone()) {
                    // Automatically select a gamepad if it's connected and keyboard is currently selected.
                    if let InputConfigurationKind::Keyboard(_) =
                        input_settings.get_selected_configuration(0).kind
                    {
                        input_settings.selected[0] = conf.id;
                    } else if let InputConfigurationKind::Keyboard(_) =
                        input_settings.get_selected_configuration(1).kind
                    {
                        input_settings.selected[1] = conf.id;
                    }
                } else {
                    log::error!("Could not setup controller {:?}", which);
                }
            }
            // Event::ControllerDeviceRemoved { which, .. } => {
            //     self.get_or_create_gamepad(which).disconnected = true;
            // }
            GamepadEvent::ButtonDown { which, button, .. } => {
                if let Some(gamepad_state) = self.get_gamepad(which.clone()) {
                    gamepad_state.toogle_button(button, true);
                } else {
                    log::warn!("Button down on unmapped gamepad {:?}", which);
                }
            }
            GamepadEvent::ButtonUp { which, button, .. } => {
                if let Some(gamepad_state) = self.get_gamepad(which.clone()) {
                    gamepad_state.toogle_button(button, false);
                } else {
                    log::warn!("Button up on unmapped gamepad {:?}", which);
                }
            }
            _ => (),
        }
    }
}
impl Sdl2Gamepads {
    fn to_gamepad_id(id: &InputId) -> String {
        format!("01-gamepad-{}", id)
    }

    pub fn new(game_controller_subsystem: GameControllerSubsystem) -> Self {
        Sdl2Gamepads {
            game_controller_subsystem,
            all: HashMap::new(),
        }
    }

    fn get_gamepad(&mut self, id: InputId) -> Option<&mut Box<dyn GamepadState>> {
        self.all.get_mut(&Self::to_gamepad_id(&id))
    }

    fn setup_gamepad_config(&mut self, input_id: InputId) -> Option<InputConfiguration> {
        if let Some(found_controller) =
            (0..self.game_controller_subsystem.num_joysticks().unwrap_or(0)).find_map(|id| {
                if input_id == id.to_input_id()
                    && self.game_controller_subsystem.is_game_controller(id)
                {
                    match self.game_controller_subsystem.open(id) {
                        Ok(c) => Some(c),
                        Err(e) => {
                            log::error!("Failed to open controller {:?}", e);
                            None
                        }
                    }
                } else {
                    None
                }
            })
        {
            let instance_id = found_controller.instance_id().to_input_id();
            let gamepad_id = Self::to_gamepad_id(&instance_id);
            self.all.insert(
                gamepad_id.clone(),
                Box::new(Sdl2GamepadState::new(found_controller)),
            );
            let input_settings = &mut Settings::current().input;
            let conf = input_settings.get_or_create_config(
                gamepad_id.clone(),
                input::InputConfiguration {
                    name: format!("🎮 Gamepad {}", instance_id),
                    id: gamepad_id,
                    kind: InputConfigurationKind::Gamepad(input_settings.default_gamepad_mapping),
                },
            );
            Some(conf.clone())
        } else {
            None
        }
    }
}

impl ToGamepadEvent for sdl2::event::Event {
    fn to_gamepad_event(&self) -> Option<GamepadEvent> {
        match self {
            sdl2::event::Event::ControllerDeviceAdded { which, .. } => {
                Some(GamepadEvent::ControllerAdded {
                    which: which.to_input_id(),
                })
            }
            sdl2::event::Event::ControllerDeviceRemoved { which, .. } => {
                Some(GamepadEvent::ControllerRemoved {
                    which: which.to_input_id(),
                })
            }
            sdl2::event::Event::ControllerButtonDown { which, button, .. } => button
                .to_gamepad_button()
                .map(|button| GamepadEvent::ButtonDown {
                    which: which.to_input_id(),
                    button,
                }),
            sdl2::event::Event::ControllerButtonUp { which, button, .. } => button
                .to_gamepad_button()
                .map(|button| GamepadEvent::ButtonUp {
                    which: which.to_input_id(),
                    button,
                }),
            _ => None,
        }
    }
}

impl ToGamepadButton for sdl2::controller::Button {
    fn to_gamepad_button(&self) -> Option<GamepadButton> {
        use sdl2::controller::Button::*;
        match self {
            A => Some(GamepadButton::A),
            B => Some(GamepadButton::B),
            X => Some(GamepadButton::X),
            Y => Some(GamepadButton::Y),
            Back => Some(GamepadButton::Back),
            Guide => Some(GamepadButton::Guide),
            Start => Some(GamepadButton::Start),
            LeftStick => Some(GamepadButton::LeftStick),
            RightStick => Some(GamepadButton::RightStick),
            LeftShoulder => Some(GamepadButton::LeftShoulder),
            RightShoulder => Some(GamepadButton::RightShoulder),
            DPadUp => Some(GamepadButton::DPadUp),
            DPadDown => Some(GamepadButton::DPadDown),
            DPadLeft => Some(GamepadButton::DPadLeft),
            DPadRight => Some(GamepadButton::DPadRight),
            Misc1 => Some(GamepadButton::Misc1),
            Paddle1 => Some(GamepadButton::Paddle1),
            Paddle2 => Some(GamepadButton::Paddle2),
            Paddle3 => Some(GamepadButton::Paddle3),
            Paddle4 => Some(GamepadButton::Paddle4),
            Touchpad => Some(GamepadButton::Touchpad),
        }
    }
}
