use crate::input::keys::{KeyCode, Modifiers};

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
                let _ = window.request_inner_size(default_window_size());
            } else {
                window.set_simple_fullscreen(true);
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            if window.is_fullscreen() {
                window.set_fullscreen(None);
                let _ = window.request_inner_size(default_window_size());
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

/// The window's default size for the region the player picked.
fn default_window_size() -> crate::Size {
    crate::integer_scaling::default_window_size(
        crate::settings::Settings::current()
            .nes_region()
            .pixel_aspect_ratio(),
    )
}
