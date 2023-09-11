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
            Key1 => Some(KeyCode::Key1),
            // The '2' key over the letters.
            Key2 => Some(KeyCode::Key2),
            // The '3' key over the letters.
            Key3 => Some(KeyCode::Key3),
            // The '4' key over the letters.
            Key4 => Some(KeyCode::Key4),
            // The '5' key over the letters.
            Key5 => Some(KeyCode::Key5),
            // The '6' key over the letters.
            Key6 => Some(KeyCode::Key6),
            // The '7' key over the letters.
            Key7 => Some(KeyCode::Key7),
            // The '8' key over the letters.
            Key8 => Some(KeyCode::Key8),
            // The '9' key over the letters.
            Key9 => Some(KeyCode::Key9),
            // The '0' key over the 'O' and 'P' keys.
            Key0 => Some(KeyCode::Key0),

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
            Snapshot => Some(KeyCode::Snapshot),
            // Scroll Lock.
            Scroll => Some(KeyCode::Scroll),
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
            Back => Some(KeyCode::Back),
            // The Enter key.
            Return => Some(KeyCode::Return),
            // The space bar.
            Space => Some(KeyCode::Space),

            // The "Compose" key on Linux.
            Compose => Some(KeyCode::Compose),

            Caret => Some(KeyCode::Caret),

            Numlock => Some(KeyCode::Numlock),
            Numpad0 => Some(KeyCode::Numpad0),
            Numpad1 => Some(KeyCode::Numpad1),
            Numpad2 => Some(KeyCode::Numpad2),
            Numpad3 => Some(KeyCode::Numpad3),
            Numpad4 => Some(KeyCode::Numpad4),
            Numpad5 => Some(KeyCode::Numpad5),
            Numpad6 => Some(KeyCode::Numpad6),
            Numpad7 => Some(KeyCode::Numpad7),
            Numpad8 => Some(KeyCode::Numpad8),
            Numpad9 => Some(KeyCode::Numpad9),
            NumpadAdd => Some(KeyCode::NumpadAdd),
            NumpadDivide => Some(KeyCode::NumpadDivide),
            NumpadDecimal => Some(KeyCode::NumpadDecimal),
            NumpadComma => Some(KeyCode::NumpadComma),
            NumpadEnter => Some(KeyCode::NumpadEnter),
            NumpadEquals => Some(KeyCode::NumpadEquals),
            NumpadMultiply => Some(KeyCode::NumpadMultiply),
            NumpadSubtract => Some(KeyCode::NumpadSubtract),

            AbntC1 => Some(KeyCode::AbntC1),
            AbntC2 => Some(KeyCode::AbntC2),
            Apostrophe => Some(KeyCode::Apostrophe),
            Apps => Some(KeyCode::Apps),
            Asterisk => Some(KeyCode::Asterisk),
            At => Some(KeyCode::At),
            Ax => Some(KeyCode::Ax),
            Backslash => Some(KeyCode::Backslash),
            Calculator => Some(KeyCode::Calculator),
            Capital => Some(KeyCode::Capital),
            Colon => Some(KeyCode::Colon),
            Comma => Some(KeyCode::Comma),
            Convert => Some(KeyCode::Convert),
            Equals => Some(KeyCode::Equals),
            Grave => Some(KeyCode::Grave),
            Kana => Some(KeyCode::Kana),
            Kanji => Some(KeyCode::Kanji),
            LAlt => Some(KeyCode::LAlt),
            LBracket => Some(KeyCode::LBracket),
            LControl => Some(KeyCode::LControl),
            LShift => Some(KeyCode::LShift),
            LWin => Some(KeyCode::LWin),
            Mail => Some(KeyCode::Mail),
            MediaSelect => Some(KeyCode::MediaSelect),
            MediaStop => Some(KeyCode::MediaStop),
            Minus => Some(KeyCode::Minus),
            Mute => Some(KeyCode::Mute),
            MyComputer => Some(KeyCode::MyComputer),
            // also called "Next"
            NavigateForward => Some(KeyCode::NavigateForward),
            // also called "Prior"
            NavigateBackward => Some(KeyCode::NavigateBackward),
            NextTrack => Some(KeyCode::NextTrack),
            NoConvert => Some(KeyCode::NoConvert),
            OEM102 => Some(KeyCode::OEM102),
            Period => Some(KeyCode::Period),
            PlayPause => Some(KeyCode::PlayPause),
            Plus => Some(KeyCode::Plus),
            Power => Some(KeyCode::Power),
            PrevTrack => Some(KeyCode::PrevTrack),
            RAlt => Some(KeyCode::RAlt),
            RBracket => Some(KeyCode::RBracket),
            RControl => Some(KeyCode::RControl),
            RShift => Some(KeyCode::RShift),
            RWin => Some(KeyCode::RWin),
            Semicolon => Some(KeyCode::Semicolon),
            Slash => Some(KeyCode::Slash),
            Sleep => Some(KeyCode::Sleep),
            Stop => Some(KeyCode::Stop),
            Sysrq => Some(KeyCode::Sysrq),
            Tab => Some(KeyCode::Tab),
            Underline => Some(KeyCode::Underline),
            Unlabeled => Some(KeyCode::Unlabeled),
            VolumeDown => Some(KeyCode::VolumeDown),
            VolumeUp => Some(KeyCode::VolumeUp),
            Wake => Some(KeyCode::Wake),
            WebBack => Some(KeyCode::WebBack),
            WebFavorites => Some(KeyCode::WebFavorites),
            WebForward => Some(KeyCode::WebForward),
            WebHome => Some(KeyCode::WebHome),
            WebRefresh => Some(KeyCode::WebRefresh),
            WebSearch => Some(KeyCode::WebSearch),
            WebStop => Some(KeyCode::WebStop),
            Yen => Some(KeyCode::Yen),
            Copy => Some(KeyCode::Copy),
            Paste => Some(KeyCode::Paste),
            Cut => Some(KeyCode::Cut),
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
