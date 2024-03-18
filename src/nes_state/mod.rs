use crate::{
    input::JoypadInput,
    settings::{gui::GuiComponent, MAX_PLAYERS},
    Fps,
};
pub static NTSC_PAL: &[u8] = include_bytes!("ntscpalette.pal");

// pub mod rusticnes;
// use self::rusticnes::RusticNesState;
// pub type LocalNesState = RusticNesState;

pub mod tetanes;
use self::tetanes::TetanesNesState;
pub type LocalNesState = TetanesNesState;

#[derive(Clone)]
pub struct FrameData {
    pub video: Vec<u8>,
    pub audio: Vec<f32>,
    pub fps: Fps,
}

pub trait NesStateHandler {
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Option<FrameData>;
    fn save(&self) -> Option<Vec<u8>>;
    fn load(&mut self, data: &mut Vec<u8>);
    fn get_gui(&mut self) -> Option<&mut dyn GuiComponent>;
    fn discard_samples(&mut self);
}
