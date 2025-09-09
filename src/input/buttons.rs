use serde::{Deserialize, Serialize};

pub trait ToGamepadButton {
    fn to_gamepad_button(&self) -> Option<GamepadButton>;
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub enum GamepadButton {
    South,
    East,
    West,
    North,

    Back,

    Start,
    Guide,

    LeftStick,
    RightStick,
    LeftShoulder,
    RightShoulder,

    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,

    Touchpad,

    LeftPaddle1,
    RightPaddle1,
    LeftPaddle2,
    RightPaddle2,

    Misc1,
    Misc2,
    Misc3,
    Misc4,
    Misc5,
}

impl std::fmt::Display for GamepadButton {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GamepadButton::South => write!(f, "South"),
            GamepadButton::East => write!(f, "East"),
            GamepadButton::West => write!(f, "West"),
            GamepadButton::North => write!(f, "North"),
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
