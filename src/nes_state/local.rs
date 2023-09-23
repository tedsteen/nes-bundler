use super::NesStateHandler;
use crate::{
    input::JoypadInput,
    settings::{gui::GuiComponent, MAX_PLAYERS},
    Fps, FPS,
};
use std::ops::{Deref, DerefMut};

pub struct LocalNesState(pub rusticnes_core::nes::NesState);

impl Deref for LocalNesState {
    type Target = rusticnes_core::nes::NesState;
    fn deref(&self) -> &rusticnes_core::nes::NesState {
        &self.0
    }
}

impl DerefMut for LocalNesState {
    fn deref_mut(&mut self) -> &mut rusticnes_core::nes::NesState {
        &mut self.0
    }
}

impl Clone for LocalNesState {
    fn clone(&self) -> Self {
        let data = &mut self.save();
        let mut clone = Self(rusticnes_core::nes::NesState::new(self.mapper.clone()));
        clone.load(data);
        clone
    }
}

impl NesStateHandler for LocalNesState {
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Fps {
        self.p1_input = *inputs[0];
        self.p2_input = *inputs[1];
        self.run_until_vblank();
        FPS
    }

    fn save(&self) -> Vec<u8> {
        self.save_state()
    }
    fn load(&mut self, data: &mut Vec<u8>) {
        self.load_state(data);
    }

    fn consume_samples(&mut self) -> Vec<i16> {
        self.apu.consume_samples()
    }

    fn get_frame(&self) -> Option<Vec<u16>> {
        Some(self.ppu.screen.clone())
    }
    fn get_gui(&mut self) -> Option<&mut dyn GuiComponent> {
        None
    }
}
