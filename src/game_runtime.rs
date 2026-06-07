use crate::app_event::AppEvent;
use crate::audio::AudioStream;
use crate::emulation::{Emulator, SharedState};
use crate::input::JoypadState;
use winit::event_loop::EventLoopProxy;

pub struct GameRuntime {
    emulator: Emulator,
}

impl GameRuntime {
    pub fn new(audio_stream: &mut AudioStream, event_proxy: EventLoopProxy<AppEvent>) -> Self {
        Self {
            emulator: Emulator::new(audio_stream, event_proxy),
        }
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
