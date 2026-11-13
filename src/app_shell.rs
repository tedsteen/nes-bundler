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
use crate::{integer_scaling, window};

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
    latency: LatencyLog,
}

/// Logs, now and then, how long a frame takes from taking the input to presenting it, and how
/// long the loop waits for the display.
struct LatencyLog {
    since: Instant,
    frames: u32,
    input_to_present: Duration,
    waiting_for_display: Duration,
}

impl LatencyLog {
    const INTERVAL: Duration = Duration::from_secs(10);

    fn new() -> Self {
        Self {
            since: Instant::now(),
            frames: 0,
            input_to_present: Duration::ZERO,
            waiting_for_display: Duration::ZERO,
        }
    }

    fn frame(&mut self, input_to_present: Duration, waiting_for_display: Duration) {
        self.frames += 1;
        self.input_to_present += input_to_present;
        self.waiting_for_display += waiting_for_display;
        if self.since.elapsed() >= Self::INTERVAL {
            log::debug!(
                "Latency: {:.1} ms from input to present, {:.1} ms waiting for the display (averages over {} frames)",
                self.input_to_present.as_secs_f32() * 1000.0 / self.frames as f32,
                self.waiting_for_display.as_secs_f32() * 1000.0 / self.frames as f32,
                self.frames
            );
            *self = Self::new();
        }
    }
}

/// Turns display refreshes into emulator frames.
///
/// Presenting waits for vsync, so the event loop runs once per display refresh, and each run ticks
/// the emulator. A display refreshing at a whole multiple of the game's frame rate (60 or 120 Hz
/// for NTSC, 50 or 100 Hz for PAL) runs exactly one frame every that many refreshes, so frames are
/// evenly spaced whatever the timing noise; the small difference (60 Hz showing 60.1 Hz NTSC, say)
/// is left to audio rate control. Other displays (144 Hz, say) get frames for the time that passed.
struct DisplayClock {
    last_tick: Instant,
    refresh: RefreshEstimate,
    /// Refreshes per frame, when locked to a whole multiple of the frame rate.
    locked: Option<u32>,
    /// Time not yet turned into refreshes or frames, so ticks that come early or late even out.
    carry: f64,
    /// Whether the last run presented, and so waited for a refresh. A hidden window runs on a
    /// timer instead, which says nothing about the display.
    paced_by_display: bool,
}

/// How long a display refresh takes, from the gaps between runs.
///
/// Runs wait for vsync, so on a fixed-rate display the gaps are whole numbers of refreshes plus a
/// little scheduling noise. Only those count: a gap that lands between refreshes (a run that
/// returned early, a variable-refresh display showing a late frame) would bias the estimate, and a
/// biased estimate makes a locked clock gain or lose frames. Should most gaps stop landing on
/// whole refreshes (a variable-refresh display not keeping up, the window moved to another
/// display), it starts over.
#[derive(Default)]
struct RefreshEstimate {
    /// The first gaps, to take a rough interval from.
    seeding: Vec<f64>,
    /// Decaying sums of the gaps that landed on whole refreshes, and of the refreshes they span.
    time: f64,
    refreshes: f64,
    /// Decaying share of gaps that didn't.
    off_grid: f64,
}

impl RefreshEstimate {
    const SEED_GAPS: usize = 15;
    /// Weighs about the last 600 gaps (5 seconds at 120 Hz).
    const DECAY: f64 = 1.0 - 1.0 / 600.0;
    /// Weighs about the last 120 gaps.
    const OFF_GRID_DECAY: f64 = 1.0 - 1.0 / 120.0;
    /// How close to a whole number of refreshes a gap has to be to count.
    const ON_GRID: f64 = 0.2;

    /// Refreshes measured before the estimate is good enough to lock to.
    const SETTLED: f64 = 120.0;

    fn interval(&self) -> Option<f64> {
        (self.refreshes > 0.0).then(|| self.time / self.refreshes)
    }

    fn settled(&self) -> bool {
        self.refreshes >= Self::SETTLED
    }

    fn add(&mut self, gap: f64) {
        let Some(interval) = self.interval() else {
            self.seeding.push(gap);
            if self.seeding.len() == Self::SEED_GAPS {
                // The median: most gaps are one refresh.
                self.seeding.sort_by(f64::total_cmp);
                self.time = self.seeding[Self::SEED_GAPS / 2];
                self.refreshes = 1.0;
                self.off_grid = 0.0;
                self.seeding.clear();
            }
            return;
        };

        let refreshes = (gap / interval).round();
        let on_grid = refreshes >= 1.0 && (gap / interval - refreshes).abs() < Self::ON_GRID;
        self.off_grid = self.off_grid * Self::OFF_GRID_DECAY
            + if on_grid {
                0.0
            } else {
                1.0 - Self::OFF_GRID_DECAY
            };
        if on_grid {
            self.time = self.time * Self::DECAY + gap;
            self.refreshes = self.refreshes * Self::DECAY + refreshes;
        } else if self.off_grid > 0.5 {
            log::debug!("Display refreshes irregularly, measuring it again");
            *self = Self::default();
        }
    }
}

impl DisplayClock {
    /// How close to a whole multiple of the game's frame rate a display has to be to lock, and
    /// how far it has to drift to unlock. Fixed-rate displays are well within (60 Hz is 0.16% off
    /// NTSC's 60.1, 59.94 Hz 0.26%), and audio rate control covers the difference.
    const LOCK_WITHIN: f64 = 0.003;
    const UNLOCK_BEYOND: f64 = 0.004;

    fn new() -> Self {
        Self {
            last_tick: Instant::now(),
            refresh: RefreshEstimate::default(),
            locked: None,
            carry: 0.0,
            paced_by_display: false,
        }
    }

    /// The whole number of refreshes per frame `interval` is close enough to.
    fn lock_for(&self, interval: f64, frame_time: f64) -> Option<u32> {
        let refreshes = (frame_time / interval).round().max(1.0);
        let off = (interval * refreshes / frame_time - 1.0).abs();
        let tolerance = if self.locked == Some(refreshes as u32) {
            Self::UNLOCK_BEYOND
        } else {
            Self::LOCK_WITHIN
        };
        (off < tolerance).then_some(refreshes as u32)
    }

    /// Hands the emulator the frames due since the last tick. `presented` is whether this run is
    /// going to present, which tells the next tick whether it waited for the display.
    fn tick(&mut self, runtime: &GameRuntime, presented: bool) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_tick).as_secs_f64();
        self.last_tick = now;

        let frame_time = runtime.frame_time().as_secs_f64();
        if self.paced_by_display && (frame_time / 8.0..frame_time * 4.0).contains(&elapsed) {
            self.refresh.add(elapsed);
        }
        self.paced_by_display = presented;

        let interval = self.refresh.interval();
        let locked = interval
            .filter(|_| self.refresh.settled())
            .and_then(|interval| self.lock_for(interval, frame_time));
        if locked != self.locked {
            self.locked = locked;
            log::debug!(
                "Display at {:.3} Hz: {}",
                interval.map_or(0.0, |interval| 1.0 / interval),
                match locked {
                    Some(1) => "one frame per refresh".to_string(),
                    Some(n) => format!("one frame every {n} refreshes"),
                    None => "frames for the time that passed".to_string(),
                }
            );
        }

        let (frames, speed) = match (self.locked, interval) {
            // Count whole refreshes, rounding off the jitter. What's left over carries, so a late
            // frame (on a variable-refresh display, say) still gets its time back.
            (Some(refreshes_per_frame), Some(interval)) if presented => {
                let refreshes = ((self.carry + elapsed) / interval).round();
                self.carry += elapsed - refreshes * interval;
                let refreshes_per_frame = refreshes_per_frame as f64;
                (
                    refreshes / refreshes_per_frame,
                    frame_time / (refreshes_per_frame * interval),
                )
            }
            _ => {
                let frames = (self.carry + elapsed) / frame_time;
                self.carry = 0.0;
                (frames, 1.0)
            }
        };

        // Long enough for a netplay rollback, short enough to still make this refresh.
        runtime.tick(
            frames as f32,
            speed as f32,
            Duration::from_secs_f64(frame_time * 0.75),
        );
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
            latency: LatencyLog::new(),
        }
    }
}

impl ApplicationHandler for AppShell {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let pixel_aspect = Settings::current().nes_region().pixel_aspect_ratio();
        let window = window::create_window(
            &self.app.config().name,
            integer_scaling::default_window_size(pixel_aspect),
            integer_scaling::min_window_size(pixel_aspect),
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

        // The texture to draw into was acquired at the end of the last run, so nothing waits
        // between taking the input above, running the emulator and presenting.
        let input_taken = Instant::now();
        self.display.tick(&self.runtime, !self.occluded);
        if !self.occluded && self.ui.render(main_view) {
            let presented = Instant::now();
            // Waiting for the frame to be on screen happens now, before returning to winit, so
            // input that comes in meanwhile is in time for the next frame.
            main_view.acquire();
            self.latency
                .frame(presented - input_taken, presented.elapsed());
            event_loop.set_control_flow(ControlFlow::Poll);
        } else {
            // A hidden window presents nothing and so doesn't wait for vsync; a timer keeps the
            // emulator going instead (netplay can't pause), until the window shows again.
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                self.display.last_tick + self.runtime.frame_time(),
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
