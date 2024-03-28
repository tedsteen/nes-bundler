use crate::{input::JoypadState, settings::MAX_PLAYERS, window::NESFrame};

pub mod emulator;
// pub mod rusticnes;
//use self::rusticnes::RusticNesState;
// pub type LocalNesState = RusticNesState;
pub mod tetanes;
use self::tetanes::TetanesNesState;
pub type LocalNesState = TetanesNesState;

#[derive(Clone)]
pub struct FrameData {
    pub audio: Vec<f32>,
}

static NTSC_PAL: &[u8] = include_bytes!("../../config/ntscpalette.pal");

pub trait NesStateHandler {
    fn advance(
        &mut self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        nes_frame: &mut Option<&mut NESFrame>,
    ) -> Option<FrameData>;
    fn save(&self) -> Option<Vec<u8>>;
    fn load(&mut self, data: &mut Vec<u8>);
    fn discard_samples(&mut self);
    fn set_speed(&mut self, speed: f32);
    fn frame(&self) -> u32;
}
