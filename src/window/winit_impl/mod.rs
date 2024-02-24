use winit::dpi::LogicalSize;

use crate::input::keys::{KeyCode, Modifiers};

use super::Fullscreen;

mod conversions;

impl Fullscreen for winit::window::Window {
    fn check_and_set_fullscreen(&mut self, key_mod: &Modifiers, key_code: &KeyCode) -> bool {
        let window = self;
        let key_mod = *key_mod;
        let key_code = *key_code;

        #[cfg(target_os = "macos")]
        if key_mod.contains(Modifiers::LOGO)
            && (key_code == KeyCode::KeyF || key_code == KeyCode::Enter)
        {
            use winit::platform::macos::WindowExtMacOS;
            if window.simple_fullscreen() {
                window.set_simple_fullscreen(false);
                let _ = window.request_inner_size(LogicalSize::new(
                    crate::MINIMUM_INTEGER_SCALING_SIZE.0,
                    crate::MINIMUM_INTEGER_SCALING_SIZE.1,
                ));
            } else {
                window.set_simple_fullscreen(true);
            }
            return true;
        }

        #[cfg(not(target_os = "macos"))]
        if (key_mod.contains(Modifiers::ALT) && key_code == KeyCode::Enter)
            || key_code == KeyCode::F11
        {
            if window.fullscreen().is_some() {
                window.set_fullscreen(None);
                let _ = window.request_inner_size(LogicalSize::new(
                    crate::MINIMUM_INTEGER_SCALING_SIZE.0,
                    crate::MINIMUM_INTEGER_SCALING_SIZE.1,
                ));
            } else {
                window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
            }
            return true;
        };

        false
    }
}
