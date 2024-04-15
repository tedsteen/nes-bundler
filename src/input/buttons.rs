use serde::{Deserialize, Serialize};

pub trait ToGamepadButton {
    fn to_gamepad_button(&self) -> Option<GamepadButton>;
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub enum GamepadButton {
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
    DPadLeft,
    DPadRight,
    Misc1,
    Paddle1,
    Paddle2,
    Paddle3,
    Paddle4,
    Touchpad,
}

impl std::fmt::Display for GamepadButton {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GamepadButton::A => write!(f, "A"),
            GamepadButton::B => write!(f, "B"),
            GamepadButton::X => write!(f, "X"),
            GamepadButton::Y => write!(f, "Y"),
            GamepadButton::Back => write!(f, "Back"),
            GamepadButton::Start => write!(f, "Start"),
            GamepadButton::LeftStick => write!(f, "Stick Left"),
            GamepadButton::RightStick => write!(f, "Stick Right"),
            GamepadButton::LeftShoulder => write!(f, "Shoulder Left"),
            GamepadButton::RightShoulder => write!(f, "Shoulder Right"),
            GamepadButton::DPadUp => write!(f, "Up"),
            GamepadButton::DPadDown => write!(f, "Down"),
            GamepadButton::DPadLeft => write!(f, "Left"),
            GamepadButton::DPadRight => write!(f, "Right"),

            //TODO: Better names for the rest?
            _ => write!(f, "{self:?}"),
        }
    }
}
