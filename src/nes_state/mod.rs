use rusticnes_core::mmc::mapper::Mapper;

use crate::{
    input::JoypadInput,
    settings::{gui::GuiComponent, MAX_PLAYERS},
    Fps,
};

use self::local::LocalNesState;

pub mod local;
#[derive(Clone)]
pub struct FrameData {
    pub video: Vec<u16>,
    pub audio: Vec<i16>,
    pub fps: Fps,
}

pub trait NesStateHandler {
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Option<FrameData>;
    fn save(&self) -> Option<Vec<u8>>;
    fn load(&mut self, data: &mut Vec<u8>);
    fn get_gui(&mut self) -> Option<&mut dyn GuiComponent>;
}

pub fn start_nes(mapper: Box<dyn Mapper>) -> LocalNesState {
    let mut nes = LocalNesState(rusticnes_core::nes::NesState::new(mapper));
    nes.power_on();
    nes
}
