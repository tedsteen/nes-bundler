use crate::{
    input::keys::{KeyCode, Modifiers},
    integer_scaling::MINIMUM_INTEGER_SCALING_SIZE,
};

use super::Fullscreen;

mod conversions;

impl Fullscreen for winit::window::Window {
    fn check_and_set_fullscreen(&self, key_mod: Modifiers, key_code: KeyCode) -> bool {
        #[cfg(target_os = "macos")]
        if key_mod.contains(Modifiers::LOGO)
            && (key_code == KeyCode::KeyF || key_code == KeyCode::Enter)
        {
            self.toggle_fullscreen();
            return true;
        }

        #[cfg(not(target_os = "macos"))]
        if (key_mod.contains(Modifiers::ALT) && key_code == KeyCode::Enter)
            || key_code == KeyCode::F11
        {
            self.toggle_fullscreen();
            return true;
        };

        false
    }

    fn toggle_fullscreen(&self) {
        let window = self;
        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::WindowExtMacOS;
            if window.is_fullscreen() {
                window.set_simple_fullscreen(false);
                let _ = window.request_inner_size(MINIMUM_INTEGER_SCALING_SIZE);
            } else {
                window.set_simple_fullscreen(true);
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            if window.is_fullscreen() {
                window.set_fullscreen(None);
                let _ = window.request_inner_size(MINIMUM_INTEGER_SCALING_SIZE);
            } else {
                window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
            }
        }
    }

    fn is_fullscreen(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::WindowExtMacOS;
            self.simple_fullscreen()
        }

        #[cfg(not(target_os = "macos"))]
        self.fullscreen().is_some()
    }
}
