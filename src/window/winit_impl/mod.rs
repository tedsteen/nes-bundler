use winit::dpi::LogicalSize;

use crate::input::keys::{KeyCode, Modifiers};

use super::Fullscreen;

mod conversions;
impl Fullscreen for winit::window::Window {
    fn check_and_set_fullscreen(&mut self, key_mod: Modifiers, key_code: KeyCode) -> bool {
        let window = self;
        #[cfg(target_os = "macos")]
        if key_mod.contains(Modifiers::LOGO)
            && (key_code == KeyCode::F || key_code == KeyCode::Return)
        {
            use winit::platform::macos::WindowExtMacOS;
            if window.simple_fullscreen() {
                window.set_simple_fullscreen(false);
                window.set_inner_size(LogicalSize::new(
                    crate::WIDTH * crate::ZOOM as u32,
                    crate::HEIGHT * crate::ZOOM as u32,
                ));
            } else {
                window.set_simple_fullscreen(true);
            }
            return true;
        }

        #[cfg(not(target_os = "macos"))]
        if (key_mod.contains(Modifiers::ALT) && key_code == KeyCode::Return)
            || key_code == KeyCode::F11
        {
            if window.fullscreen().is_some() {
                window.set_fullscreen(None);
                window.set_inner_size(LogicalSize::new(WIDTH * ZOOM as u32, HEIGHT * ZOOM as u32));
            } else {
                window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
            }
            return true;
        };

        false
    }
}
