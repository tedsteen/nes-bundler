use crate::{input::JoypadState, settings::MAX_PLAYERS, Fps, NES_HEIGHT, NES_WIDTH};

use self::rusticnes::RusticNesState;

pub mod emulator;
pub mod rusticnes;
pub type LocalNesState = RusticNesState;

#[derive(Clone)]
pub struct FrameData {
    pub audio: Vec<f32>,
    pub fps: Fps,
}
pub type VideoFrame = [u8; (NES_WIDTH * NES_HEIGHT * 3) as usize];

pub trait NesStateHandler: Send {
    fn advance(
        &mut self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        video_frame: &mut Option<&mut VideoFrame>,
    ) -> Option<FrameData>;
    fn save(&self) -> Option<Vec<u8>>;
    fn load(&mut self, data: &mut Vec<u8>);
    fn discard_samples(&mut self);
}
