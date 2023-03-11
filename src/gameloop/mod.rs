use time::TimeTrait;

mod time;
pub struct GameLoop<G, T: TimeTrait, W> {
    pub game: G,
    pub updates_per_second: u32,
    pub max_frame_time: f64,
    pub exit_next_iteration: bool,
    pub window: W,

    fixed_time_step: f64,
    number_of_updates: u32,
    number_of_renders: u32,
    last_frame_time: f64,
    running_time: f64,
    accumulated_time: f64,
    blending_factor: f64,
    previous_instant: T,
    current_instant: T,
}

impl<G, T: TimeTrait, W> GameLoop<G, T, W> {
    pub fn new(game: G, updates_per_second: u32, max_frame_time: f64, window: W) -> Self {
        Self {
            game,
            updates_per_second,
            max_frame_time,
            window,
            exit_next_iteration: false,

            fixed_time_step: 1.0 / updates_per_second as f64,
            number_of_updates: 0,
            number_of_renders: 0,
            running_time: 0.0,
            accumulated_time: 0.0,
            blending_factor: 0.0,
            previous_instant: T::now(),
            current_instant: T::now(),
            last_frame_time: 0.0,
        }
    }

    pub fn next_frame<U, R>(&mut self, mut update: U, mut render: R) -> bool
    where
        U: FnMut(&mut GameLoop<G, T, W>),
        R: FnMut(&mut GameLoop<G, T, W>),
    {
        let mut g = self;

        if g.exit_next_iteration {
            return false;
        }

        g.current_instant = T::now();

        let mut elapsed = g.current_instant.sub(&g.previous_instant);

        if elapsed > g.max_frame_time {
            elapsed = g.max_frame_time;
        }

        g.last_frame_time = elapsed;
        g.running_time += elapsed;
        g.accumulated_time += elapsed;

        while g.accumulated_time >= g.fixed_time_step {
            update(g);

            g.accumulated_time -= g.fixed_time_step;
            g.number_of_updates += 1;
        }

        g.blending_factor = g.accumulated_time / g.fixed_time_step;

        render(g);

        g.number_of_renders += 1;
        g.previous_instant = g.current_instant;

        true
    }

    pub fn exit(&mut self) {
        self.exit_next_iteration = true;
    }

    pub fn set_updates_per_second(&mut self, new_updates_per_second: u32) {
        self.updates_per_second = new_updates_per_second;
        self.fixed_time_step = 1.0 / new_updates_per_second as f64;
    }
}

use winit::event::Event;
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;

pub use winit;

use self::time::Time;

#[allow(clippy::too_many_arguments)]
pub fn game_loop<G, U, R, H, T>(
    event_loop: EventLoop<T>,
    window: Window,
    game: G,
    updates_per_second: u32,
    max_frame_time: f64,
    mut update: U,
    mut render: R,
    mut handler: H,
) -> !
where
    G: 'static,
    U: FnMut(&mut GameLoop<G, Time, Window>) + 'static,
    R: FnMut(&mut GameLoop<G, Time, Window>) + 'static,
    H: FnMut(&mut GameLoop<G, Time, Window>, &Event<'_, T>) + 'static,
    T: 'static,
{
    let mut game_loop = GameLoop::new(game, updates_per_second, max_frame_time, window);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        // Forward events to existing handlers.
        handler(&mut game_loop, &event);

        match event {
            Event::RedrawRequested(_) => {
                if !game_loop.next_frame(&mut update, &mut render) {
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::MainEventsCleared => {
                game_loop.window.request_redraw();
            }
            _ => {}
        }
    })
}
