use winit::event::WindowEvent;

use crate::{
    input::{
        keys::{Modifiers, ToGuiKeyCode, ToGuiMod},
        KeyEvent,
    },
    settings::gui::{GuiEvent, ToGuiEvent},
};
impl ToGuiKeyCode for winit::event::VirtualKeyCode {
    fn to_gui_key_code(&self) -> Option<crate::input::keys::KeyCode> {
        use crate::input::keys::KeyCode;
        use winit::event::VirtualKeyCode::*;
        match self {
            // The '1' key over the letters.
            Key1 => Some(KeyCode::Num1),
            // The '2' key over the letters.
            Key2 => Some(KeyCode::Num2),
            // The '3' key over the letters.
            Key3 => Some(KeyCode::Num3),
            // The '4' key over the letters.
            Key4 => Some(KeyCode::Num4),
            // The '5' key over the letters.
            Key5 => Some(KeyCode::Num5),
            // The '6' key over the letters.
            Key6 => Some(KeyCode::Num6),
            // The '7' key over the letters.
            Key7 => Some(KeyCode::Num7),
            // The '8' key over the letters.
            Key8 => Some(KeyCode::Num8),
            // The '9' key over the letters.
            Key9 => Some(KeyCode::Num9),
            // The '0' key over the 'O' and 'P' keys.
            Key0 => Some(KeyCode::Num0),

            A => Some(KeyCode::A),
            B => Some(KeyCode::B),
            C => Some(KeyCode::C),
            D => Some(KeyCode::D),
            E => Some(KeyCode::E),
            F => Some(KeyCode::F),
            G => Some(KeyCode::G),
            H => Some(KeyCode::H),
            I => Some(KeyCode::I),
            J => Some(KeyCode::J),
            K => Some(KeyCode::K),
            L => Some(KeyCode::L),
            M => Some(KeyCode::M),
            N => Some(KeyCode::N),
            O => Some(KeyCode::O),
            P => Some(KeyCode::P),
            Q => Some(KeyCode::Q),
            R => Some(KeyCode::R),
            S => Some(KeyCode::S),
            T => Some(KeyCode::T),
            U => Some(KeyCode::U),
            V => Some(KeyCode::V),
            W => Some(KeyCode::W),
            X => Some(KeyCode::X),
            Y => Some(KeyCode::Y),
            Z => Some(KeyCode::Z),

            // The Escape key, next to F1.
            Escape => Some(KeyCode::Escape),

            F1 => Some(KeyCode::F1),
            F2 => Some(KeyCode::F2),
            F3 => Some(KeyCode::F3),
            F4 => Some(KeyCode::F4),
            F5 => Some(KeyCode::F5),
            F6 => Some(KeyCode::F6),
            F7 => Some(KeyCode::F7),
            F8 => Some(KeyCode::F8),
            F9 => Some(KeyCode::F9),
            F10 => Some(KeyCode::F10),
            F11 => Some(KeyCode::F11),
            F12 => Some(KeyCode::F12),
            F13 => Some(KeyCode::F13),
            F14 => Some(KeyCode::F14),
            F15 => Some(KeyCode::F15),
            F16 => Some(KeyCode::F16),
            F17 => Some(KeyCode::F17),
            F18 => Some(KeyCode::F18),
            F19 => Some(KeyCode::F19),
            F20 => Some(KeyCode::F20),
            F21 => Some(KeyCode::F21),
            F22 => Some(KeyCode::F22),
            F23 => Some(KeyCode::F23),
            F24 => Some(KeyCode::F24),

            // Print Screen/SysRq.
            Snapshot => Some(KeyCode::PrintScreen),
            // Scroll Lock.
            Scroll => Some(KeyCode::ScrollLock),
            // Pause/Break key, next to Scroll lock.
            Pause => Some(KeyCode::Pause),

            // `Insert`, next to Backspace.
            Insert => Some(KeyCode::Insert),
            Home => Some(KeyCode::Home),
            Delete => Some(KeyCode::Delete),
            End => Some(KeyCode::End),
            PageDown => Some(KeyCode::PageDown),
            PageUp => Some(KeyCode::PageUp),

            Left => Some(KeyCode::Left),
            Up => Some(KeyCode::Up),
            Right => Some(KeyCode::Right),
            Down => Some(KeyCode::Down),

            // The Backspace key, right over Enter.
            Back => Some(KeyCode::Backspace),
            // The Enter key.
            Return => Some(KeyCode::Return),
            // The space bar.
            Space => Some(KeyCode::Space),

            //Compose => None,
            Caret => Some(KeyCode::Caret),

            Numlock => Some(KeyCode::NumLockClear),
            Numpad0 => Some(KeyCode::Kp0),
            Numpad1 => Some(KeyCode::Kp1),
            Numpad2 => Some(KeyCode::Kp2),
            Numpad3 => Some(KeyCode::Kp3),
            Numpad4 => Some(KeyCode::Kp4),
            Numpad5 => Some(KeyCode::Kp5),
            Numpad6 => Some(KeyCode::Kp6),
            Numpad7 => Some(KeyCode::Kp7),
            Numpad8 => Some(KeyCode::Kp8),
            Numpad9 => Some(KeyCode::Kp9),
            NumpadAdd => Some(KeyCode::KpPlus),
            NumpadDivide => Some(KeyCode::KpDivide),
            NumpadDecimal => Some(KeyCode::KpDecimal),
            NumpadComma => Some(KeyCode::KpComma),
            NumpadEnter => Some(KeyCode::KpEnter),
            NumpadEquals => Some(KeyCode::KpEquals),
            NumpadMultiply => Some(KeyCode::KpMultiply),
            NumpadSubtract => Some(KeyCode::KpMinus),

            _ => None,
            // AbntC1,
            // AbntC2,
            // Apostrophe,
            // Apps,
            // Asterisk,
            // At,
            // Ax,
            // Backslash,
            // Calculator,
            // Capital,
            // Colon,
            // Comma,
            // Convert,
            // Equals,
            // Grave,
            // Kana,
            // Kanji,
            // LAlt,
            // LBracket,
            // LControl,
            // LShift,
            // LWin,
            // Mail,
            // MediaSelect,
            // MediaStop,
            // Minus,
            // Mute,
            // MyComputer,
            // // also called "Next"
            // NavigateForward,
            // // also called "Prior"
            // NavigateBackward,
            // NextTrack,
            // NoConvert,
            // OEM102,
            // Period,
            // PlayPause,
            // Plus,
            // Power,
            // PrevTrack,
            // RAlt,
            // RBracket,
            // RControl,
            // RShift,
            // RWin,
            // Semicolon,
            // Slash,
            // Sleep,
            // Stop,
            // Sysrq,
            // Tab,
            // Underline,
            // Unlabeled,
            // VolumeDown,
            // VolumeUp,
            // Wake,
            // WebBack,
            // WebFavorites,
            // WebForward,
            // WebHome,
            // WebRefresh,
            // WebSearch,
            // WebStop,
            // Yen,
            // Copy,
            // Paste,
            // Cut,
        }
    }
}
impl ToGuiEvent for WindowEvent<'_> {
    fn to_gui_event(&self) -> Option<GuiEvent> {
        match self {
            #[allow(deprecated)] //We'll deal with this when we have to
            winit::event::WindowEvent::KeyboardInput {
                input:
                    winit::event::KeyboardInput {
                        state,
                        virtual_keycode: Some(keycode),
                        modifiers,
                        ..
                    },
                ..
            } => {
                let gui_key_code = keycode.to_gui_key_code();
                if let Some(gui_key_code) = gui_key_code {
                    use winit::event::ElementState::*;

                    Some(GuiEvent::Keyboard(match state {
                        Pressed => KeyEvent::Pressed(
                            gui_key_code,
                            modifiers.to_gui_mod().unwrap_or_default(),
                        ),
                        Released => KeyEvent::Released(
                            gui_key_code,
                            modifiers.to_gui_mod().unwrap_or_default(),
                        ),
                    }))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl ToGuiMod for winit::event::ModifiersState {
    fn to_gui_mod(&self) -> Option<Modifiers> {
        Modifiers::from_bits(self.bits())
    }
}
