use std::time::Duration;

use crate::audio::AudioStream;
use crate::emulation::{Emulator, SharedState};
use crate::input::JoypadState;

pub struct GameRuntime {
    emulator: Emulator,
}

impl GameRuntime {
    pub fn new(audio_stream: &mut AudioStream) -> Self {
        Self {
            emulator: Emulator::new(audio_stream),
        }
    }

    /// Runs `frames` frames' worth of emulation, waiting up to `timeout` for it to finish. The
    /// display runs the game at `speed` times its own frame rate.
    pub fn tick(&self, frames: f32, speed: f32, timeout: Duration) {
        self.emulator
            .shared_state
            .emulator
            .tick(frames, speed, timeout);
    }

    pub fn shared_state(&self) -> SharedState {
        self.emulator.shared_state.clone()
    }

    pub fn frame_buffer(&self) -> crate::emulation::VideoBufferPool {
        self.emulator.shared_state.emulator.frame_buffer.clone()
    }

    pub fn write_inputs(&self, joypads: [JoypadState; 2]) {
        self.emulator.shared_state.emulator.write_inputs(joypads);
    }
}
