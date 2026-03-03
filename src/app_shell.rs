use sdl3::EventPump;
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};

use crate::app_context::AppContext;
use crate::game_runtime::GameRuntime;
use crate::input::gamepad::ToGamepadEvent;
use crate::main_view::MainView;
use crate::main_view::gui::GuiEvent;
use crate::ui_controller::UiController;
use crate::window::Fullscreen;
use crate::{Size, emulation, integer_scaling, window};

pub struct AppShell {
    app: &'static AppContext,
    main_view: Option<MainView>,
    runtime: GameRuntime,
    sdl_event_pump: EventPump,
    ui: UiController,
}

impl AppShell {
    pub fn new(
        app: &'static AppContext,
        runtime: GameRuntime,
        sdl_event_pump: EventPump,
        ui: UiController,
    ) -> Self {
        Self {
            app,
            main_view: None,
            runtime,
            sdl_event_pump,
            ui,
        }
    }
}

impl ApplicationHandler for AppShell {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window = window::create_window(
            &self.app.config().name,
            integer_scaling::MINIMUM_INTEGER_SCALING_SIZE,
            Size::new(emulation::NES_WIDTH_4_3, emulation::NES_HEIGHT),
            event_loop,
        )
        .expect("a window to be created");

        self.main_view = Some(MainView::new(
            window,
            self.runtime.frame_buffer(),
            self.app.config().enable_vsync,
        ));
    }

    fn new_events(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop, cause: StartCause) {
        if let Some(main_view) = &self.main_view {
            if cause == StartCause::Init && self.app.config().start_in_fullscreen {
                main_view.window.toggle_fullscreen();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        window_event: WindowEvent,
    ) {
        if let Some(main_view) = &mut self.main_view {
            match window_event {
                WindowEvent::CloseRequested | WindowEvent::Destroyed => event_loop.exit(),
                WindowEvent::RedrawRequested => {
                    self.ui.render(main_view);
                    main_view.window.request_redraw();
                }
                _ => {}
            }

            for sdl_gui_event in self
                .sdl_event_pump
                .poll_iter()
                .flat_map(|e| e.to_gamepad_event())
                .map(GuiEvent::Gamepad)
            {
                self.ui.handle_gui_event(main_view, &sdl_gui_event);
            }

            self.runtime.write_inputs(self.ui.current_game_inputs());
            self.ui.handle_window_event(main_view, &window_event);
            if self.ui.take_exit_requested() {
                event_loop.exit();
                return;
            }
            self.ui.update_cursor_visibility(main_view);
        }
    }
}
