use std::time::{Duration, Instant};

use sdl3::EventPump;
use sdl3::event::Event;
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::ControlFlow;

use crate::app_context::AppContext;
use crate::app_event::AppEvent;
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
    redraw: RedrawScheduler,
}

struct RedrawScheduler {
    min_interval: Duration,
    next_ui_redraw: Instant,
}

impl RedrawScheduler {
    fn new(min_interval: Duration) -> Self {
        Self {
            min_interval,
            next_ui_redraw: Instant::now(),
        }
    }

    fn rendered(&mut self) {
        self.next_ui_redraw = Instant::now() + self.min_interval;
    }

    fn request_if_due(&mut self, window: &winit::window::Window) {
        let now = Instant::now();
        if now >= self.next_ui_redraw {
            window.request_redraw();
            self.next_ui_redraw = now + self.min_interval;
        }
    }

    fn apply_control_flow(&self, event_loop: &winit::event_loop::ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_ui_redraw));
    }
}

impl AppShell {
    const UI_REDRAW_INTERVAL: Duration = Duration::from_millis(33);

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
            redraw: RedrawScheduler::new(Self::UI_REDRAW_INTERVAL),
        }
    }
}

impl ApplicationHandler<AppEvent> for AppShell {
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
        if let Some(main_view) = &self.main_view
            && cause == StartCause::Init
            && self.app.config().start_in_fullscreen
        {
            main_view.window.toggle_fullscreen();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        window_event: WindowEvent,
    ) {
        let Some(main_view) = &mut self.main_view else {
            return;
        };

        Self::handle_window_action(event_loop, &mut self.ui, main_view, &window_event);

        self.runtime.write_inputs(self.ui.current_game_inputs());
        self.ui.handle_window_event(main_view, &window_event);
        if self.ui.take_exit_requested() {
            event_loop.exit();
            return;
        }
        self.ui.update_cursor_visibility(main_view);

        if matches!(window_event, WindowEvent::RedrawRequested) {
            self.redraw.rendered();
        } else {
            main_view.window.request_redraw();
        }
    }

    fn user_event(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop, event: AppEvent) {
        let Some(main_view) = &self.main_view else {
            return;
        };

        match event {
            AppEvent::NesFrameReady => main_view.window.request_redraw(),
        }
    }

    // Runs after every batch of events. The loop waits until the next UI redraw is due, but the
    // emulator wakes it for every new NES frame, so SDL events (gamepads, the quit request that
    // SIGINT/SIGTERM turn into) are handled promptly even while the window gets no events.
    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let Some(main_view) = &mut self.main_view else {
            return;
        };

        let sdl_events =
            Self::forward_sdl_events(&mut self.sdl_event_pump, &mut self.ui, main_view);
        if sdl_events.quit {
            log::info!("Quit requested, shutting down");
            event_loop.exit();
            return;
        }
        if sdl_events.handled_gamepad_events {
            main_view.window.request_redraw();
        }

        self.runtime.write_inputs(self.ui.current_game_inputs());
        if self.ui.take_exit_requested() {
            event_loop.exit();
            return;
        }
        self.ui.update_cursor_visibility(main_view);

        self.redraw.request_if_due(&main_view.window);
        self.redraw.apply_control_flow(event_loop);
    }
}

struct SdlEvents {
    /// SDL asked the app to quit, which it does on SIGINT/SIGTERM (the window belongs to winit).
    quit: bool,
    handled_gamepad_events: bool,
}

impl AppShell {
    fn handle_window_action(
        event_loop: &winit::event_loop::ActiveEventLoop,
        ui: &mut UiController,
        main_view: &mut MainView,
        window_event: &WindowEvent,
    ) {
        match window_event {
            WindowEvent::CloseRequested | WindowEvent::Destroyed => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                ui.render(main_view);
            }
            _ => {}
        }
    }

    /// Forwards pending SDL gamepad events to the UI.
    fn forward_sdl_events(
        sdl_event_pump: &mut EventPump,
        ui: &mut UiController,
        main_view: &mut MainView,
    ) -> SdlEvents {
        let mut quit = false;
        let mut handled_gamepad_events = false;
        for event in sdl_event_pump.poll_iter() {
            if let Event::Quit { .. } = event {
                quit = true;
            } else if let Some(gamepad_event) = event.to_gamepad_event() {
                handled_gamepad_events = true;
                ui.handle_gui_event(main_view, &GuiEvent::Gamepad(gamepad_event));
            }
        }
        SdlEvents {
            quit,
            handled_gamepad_events,
        }
    }
}
