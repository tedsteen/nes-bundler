use std::time::{Duration, Instant};

use sdl3::EventPump;
use sdl3::event::Event;
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::ControlFlow;

use crate::app_context::AppContext;
use crate::game_runtime::GameRuntime;
use crate::input::gamepad::ToGamepadEvent;
use crate::main_view::MainView;
use crate::main_view::gui::GuiEvent;
use crate::settings::Settings;
use crate::ui_controller::UiController;
use crate::window::Fullscreen;
use crate::{Size, emulation, integer_scaling, window};

pub struct AppShell {
    app: &'static AppContext,
    main_view: Option<MainView>,
    runtime: GameRuntime,
    sdl_event_pump: EventPump,
    ui: UiController,
    display: DisplayClock,
    /// Hidden (behind other windows, minimized). Presenting would block, waiting for a frame
    /// macOS won't show.
    occluded: bool,
}

/// Turns display refreshes into emulator frames.
///
/// Presenting waits for vsync, so the event loop runs once per display refresh, and each run ticks
/// the emulator. A display running at the game's frame rate gets exactly one frame per refresh;
/// the small difference (a 60 Hz display showing 60.1 Hz NTSC, say) is left to audio rate control
/// rather than showing up as an occasional doubled frame. Other displays (120 Hz, variable
/// refresh) get frames for the time that passed.
struct DisplayClock {
    last_tick: Instant,
    /// Smoothed time between refreshes.
    refresh_interval: Option<f32>,
    /// Running one frame per refresh.
    locked: bool,
    /// Time not yet turned into frames, so ticks that come early or late even out.
    carry: f32,
}

impl DisplayClock {
    /// How close to the game's frame rate a display has to get to run one frame per refresh, and
    /// how far it has to drift to stop. The gap keeps jitter from flipping between the two.
    const LOCK_WITHIN: f32 = 0.015;
    const UNLOCK_BEYOND: f32 = 0.03;

    fn new() -> Self {
        Self {
            last_tick: Instant::now(),
            refresh_interval: None,
            locked: false,
            carry: 0.0,
        }
    }

    fn frame_time() -> Duration {
        Duration::from_secs_f32(1.0 / Settings::current().nes_region().to_fps())
    }

    /// Hands the emulator the frames due since the last tick.
    fn tick(&mut self, runtime: &GameRuntime) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_tick).as_secs_f32();
        self.last_tick = now;

        let frame_time = Self::frame_time().as_secs_f32();
        if (frame_time / 4.0..frame_time * 4.0).contains(&elapsed) {
            self.refresh_interval = Some(
                self.refresh_interval
                    .map_or(elapsed, |interval| interval + (elapsed - interval) * 0.05),
            );
        }
        if let Some(interval) = self.refresh_interval {
            let off = (interval / frame_time - 1.0).abs();
            let locked = if self.locked {
                off < Self::UNLOCK_BEYOND
            } else {
                off < Self::LOCK_WITHIN
            };
            if locked != self.locked {
                self.locked = locked;
                self.carry = 0.0;
                log::debug!(
                    "Display at {:.2} Hz: {}",
                    1.0 / interval,
                    if locked {
                        "one frame per refresh"
                    } else {
                        "frames for the time that passed"
                    }
                );
            }
        }

        // Locked, a refresh is a frame: time is counted in refreshes, rounding off the jitter.
        let unit = match (self.locked, self.refresh_interval) {
            (true, Some(interval)) => interval,
            _ => frame_time,
        };
        let due = (self.carry + elapsed) / unit;
        let frames = if self.locked { due.round() } else { due };
        self.carry = (due - frames) * unit;

        // Long enough for a netplay rollback, short enough to still make this refresh.
        runtime.tick(frames, Duration::from_secs_f32(frame_time * 0.75));
    }
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
            display: DisplayClock::new(),
            occluded: false,
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

        self.main_view = Some(MainView::new(window, self.runtime.frame_buffer()));
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

        match &window_event {
            WindowEvent::CloseRequested | WindowEvent::Destroyed => event_loop.exit(),
            WindowEvent::Occluded(occluded) => {
                log::debug!("Window {}", if *occluded { "hidden" } else { "shown" });
                self.occluded = *occluded;
            }
            // Rendering happens in `about_to_wait`, once per refresh.
            _ => {}
        }

        self.runtime.write_inputs(self.ui.current_game_inputs());
        self.ui.handle_window_event(main_view, &window_event);
        if self.ui.take_exit_requested() {
            event_loop.exit();
            return;
        }
        self.ui.update_cursor_visibility(main_view);
    }

    // Runs after every batch of events. Renders here rather than on `RedrawRequested`: presenting
    // waits for vsync, so the loop runs exactly once per display refresh, where winit's own redraw
    // scheduling on top of that would skip refreshes.
    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let Some(main_view) = &mut self.main_view else {
            return;
        };

        if Self::forward_sdl_events(&mut self.sdl_event_pump, &mut self.ui, main_view) {
            log::info!("Quit requested, shutting down");
            event_loop.exit();
            return;
        }

        self.runtime.write_inputs(self.ui.current_game_inputs());
        if self.ui.take_exit_requested() {
            event_loop.exit();
            return;
        }
        self.ui.update_cursor_visibility(main_view);

        self.display.tick(&self.runtime);
        if !self.occluded && self.ui.render(main_view) {
            event_loop.set_control_flow(ControlFlow::Poll);
        } else {
            // A hidden window presents nothing and so doesn't wait for vsync; a timer keeps the
            // emulator going instead (netplay can't pause), until the window shows again.
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                self.display.last_tick + DisplayClock::frame_time(),
            ));
        }
    }
}

impl AppShell {
    /// Forwards pending SDL gamepad events to the UI. Returns `true` if SDL asked the app to quit,
    /// which it does on SIGINT/SIGTERM (the window belongs to winit, not SDL).
    fn forward_sdl_events(
        sdl_event_pump: &mut EventPump,
        ui: &mut UiController,
        main_view: &mut MainView,
    ) -> bool {
        let mut quit = false;
        for event in sdl_event_pump.poll_iter() {
            if let Event::Quit { .. } = event {
                quit = true;
            } else if let Some(gamepad_event) = event.to_gamepad_event() {
                ui.handle_gui_event(main_view, &GuiEvent::Gamepad(gamepad_event));
            }
        }
        quit
    }
}
