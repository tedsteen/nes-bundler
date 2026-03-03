use crate::audio::AudioStream;
use crate::emulation::{Emulator, SharedState};
use crate::input::JoypadState;

pub struct GameRuntime {
    emulator: Emulator,
}

impl GameRuntime {
    /// Ordering used when the main thread *writes* inputs consumed by the emulator thread.
    /// Paired with `Acquire` loads in `Emulator::read_input_states`.
    const INPUT_WRITE_ORDERING: std::sync::atomic::Ordering = std::sync::atomic::Ordering::Release;

    pub fn new(audio_stream: &mut AudioStream) -> Self {
        Self {
            emulator: Emulator::new(audio_stream),
        }
    }

    pub fn shared_state(&self) -> SharedState {
        self.emulator.shared_state.clone()
    }

    pub fn frame_buffer(&self) -> crate::emulation::VideoBufferPool {
        self.emulator.shared_state.emulator.frame_buffer.clone()
    }

    pub fn write_inputs(&self, joypads: [JoypadState; 2]) {
        for (idx, joypad_state) in joypads.into_iter().enumerate() {
            self.emulator.shared_state.emulator.inputs[idx]
                .store(*joypad_state, Self::INPUT_WRITE_ORDERING);
        }
    }
}
