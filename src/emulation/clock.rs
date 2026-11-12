//! How the display drives the emulator.
//!
//! Every display refresh the render loop hands the emulator thread some frames to run, as
//! credit, then waits briefly for it to finish so the new frame makes it onto that refresh.

use std::{
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use tokio::sync::Notify;

#[derive(Default)]
struct State {
    /// Frames handed out and not yet taken by the emulator thread.
    credit: f32,
    /// Ticks handed out, and the last one the emulator thread has finished.
    ticks: u64,
    done: u64,
}

#[derive(Default)]
struct Inner {
    state: Mutex<State>,
    finished: Condvar,
    wake: Notify,
}

#[derive(Clone, Default)]
pub struct FrameClock(Arc<Inner>);

impl FrameClock {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.0.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Hands the emulator `frames` to run and waits up to `timeout` for it to run them.
    pub fn tick(&self, frames: f32, timeout: Duration) {
        let tick = {
            let mut state = self.lock();
            state.credit += frames;
            state.ticks += 1;
            state.ticks
        };
        self.0.wake.notify_one();

        let state = self.lock();
        let (_state, wait) = self
            .0
            .finished
            .wait_timeout_while(state, timeout, |state| state.done < tick)
            .unwrap_or_else(PoisonError::into_inner);
        if wait.timed_out() {
            log::trace!("Emulator didn't finish its frames in {timeout:?}");
        }
    }

    /// Wakes the emulator thread without handing out frames, e.g. to shut down.
    pub fn wake(&self) {
        self.0.wake.notify_one();
    }

    /// Emulator thread: waits for a tick (or a wake-up), then takes the credit handed out so far.
    pub async fn take(&self) -> Credit {
        self.0.wake.notified().await;
        let mut state = self.lock();
        Credit {
            frames: std::mem::take(&mut state.credit),
            tick: state.ticks,
        }
    }

    /// Emulator thread: done with the frames of `credit`.
    pub fn finished(&self, credit: &Credit) {
        let mut state = self.lock();
        state.done = state.done.max(credit.tick);
        self.0.finished.notify_all();
    }
}

pub struct Credit {
    pub frames: f32,
    tick: u64,
}
