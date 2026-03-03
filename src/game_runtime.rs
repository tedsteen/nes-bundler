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

    pub fn shared_state(&self) -> SharedState {
        self.emulator.shared_state.clone()
    }

    pub fn frame_buffer(&self) -> crate::emulation::VideoBufferPool {
        self.emulator.shared_state.emulator.frame_buffer.clone()
    }

    pub fn write_inputs(&self, joypads: [JoypadState; 2]) {
        self.emulator.shared_state.emulator.inputs[0]
            .store(*joypads[0], std::sync::atomic::Ordering::Relaxed);
        self.emulator.shared_state.emulator.inputs[1]
            .store(*joypads[1], std::sync::atomic::Ordering::Relaxed);
    }
}
