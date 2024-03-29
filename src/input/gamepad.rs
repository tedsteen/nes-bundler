use super::{buttons::GamepadButton, settings::InputSettings, InputId, JoypadInput, JoypadMapping};
use std::collections::HashSet;

pub type JoypadGamepadMapping = JoypadMapping<GamepadButton>;

pub trait GamepadState {
    fn is_connected(&self) -> bool;
    fn get_pressed_buttons(&self) -> &HashSet<GamepadButton>;
    fn toogle_button(&mut self, button: &GamepadButton, on: bool);
}

pub trait Gamepads {
    fn advance(&mut self, gamepad_event: &GamepadEvent, input_settings: &mut InputSettings);
    fn get_joypad(&mut self, id: &InputId, mapping: &JoypadGamepadMapping) -> JoypadInput;
    fn get_gamepad_by_input_id(&self, id: &InputId) -> Option<&dyn GamepadState>;
}

#[derive(Clone, Debug)]
pub enum GamepadEvent {
    ControllerAdded {
        which: InputId,
    },
    ControllerRemoved {
        which: InputId,
    },
    ButtonDown {
        which: InputId,
        button: GamepadButton,
    },
    ButtonUp {
        which: InputId,
        button: GamepadButton,
    },
}

pub trait ToGamepadEvent {
    fn to_gamepad_event(&self) -> Option<GamepadEvent>;
}
