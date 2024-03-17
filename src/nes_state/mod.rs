use tetanes_core::control_deck::ControlDeck;

use crate::{
    input::JoypadInput,
    settings::{gui::GuiComponent, MAX_PLAYERS},
    Fps,
};

//pub mod rusticnes;
pub mod tetanes;

pub struct NesState<T>(pub T);

pub type LocalNesState = NesState<ControlDeck>;

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
