use std::time::{Duration, Instant};

use crate::input::JoypadState;
use crate::main_view::MainView;
use crate::main_view::gui::{GuiEvent, MainGui};
use crate::settings::MAX_PLAYERS;
use crate::window::Fullscreen;

pub struct UiController {
    pub main_gui: MainGui,
    last_mouse_touch: Instant,
    mouse_hide_timeout: Duration,
}

impl UiController {
    pub fn new(main_gui: MainGui, mouse_hide_timeout: Duration) -> Self {
        Self {
            main_gui,
            mouse_hide_timeout,
            last_mouse_touch: Instant::now()
                .checked_sub(mouse_hide_timeout)
                .expect("there to be an instant `mouse_hide_timeout` seconds in the past"),
        }
    }

    pub fn render(&mut self, main_view: &mut MainView) {
        main_view.render(&mut self.main_gui);
    }

    pub fn handle_gui_event(&mut self, main_view: &mut MainView, gui_event: &GuiEvent) {
        main_view.handle_gui_event(gui_event, &mut self.main_gui);
    }

    pub fn handle_window_event(
        &mut self,
        main_view: &mut MainView,
        window_event: &winit::event::WindowEvent,
    ) {
        if matches!(
            window_event,
            winit::event::WindowEvent::MouseInput { .. }
                | winit::event::WindowEvent::CursorMoved { .. }
        ) {
            self.last_mouse_touch = Instant::now();
        }

        main_view.handle_window_event(window_event, &mut self.main_gui);
    }

    pub fn current_game_inputs(&self) -> [JoypadState; MAX_PLAYERS] {
        self.main_gui.game_inputs()
    }

    pub fn update_cursor_visibility(&self, main_view: &MainView) {
        main_view.window.set_cursor_visible(
            !(main_view.window.is_fullscreen()
                && !self.main_gui.visible()
                && Instant::now()
                    .duration_since(self.last_mouse_touch)
                    .gt(&self.mouse_hide_timeout)),
        );
    }

    pub fn take_exit_requested(&mut self) -> bool {
        self.main_gui.take_exit_requested()
    }
}
