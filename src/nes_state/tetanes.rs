use anyhow::Context;
use tetanes_core::{self, common::Reset};

use super::{FrameData, LocalNesState, NesState, NesStateHandler};
use crate::{
    input::JoypadInput,
    settings::{gui::GuiComponent, MAX_PLAYERS},
    FPS,
};
use std::ops::{Deref, DerefMut};
type InternalNesState = tetanes_core::control_deck::ControlDeck;

impl LocalNesState {
    pub fn load_rom(rom: &[u8]) -> LocalNesState {
        let mut nes = NesState(InternalNesState::new());

        //nes.load_rom("Name", &mut rom.clone(), None);
        nes.reset(tetanes_core::common::ResetKind::Hard);

        nes
    }
}
impl Clone for LocalNesState {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl Deref for LocalNesState {
    type Target = InternalNesState;
    fn deref(&self) -> &InternalNesState {
        &self.0
    }
}

impl DerefMut for LocalNesState {
    fn deref_mut(&mut self) -> &mut InternalNesState {
        &mut self.0
    }
}

impl NesStateHandler for LocalNesState {
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Option<FrameData> {
        println!("TODO: Advance");
        // self.p1_input = *inputs[0];
        // self.p2_input = *inputs[1];
        // self.run_until_vblank();
        // Some(FrameData {
        //     video: self.ppu.screen.clone(),
        //     audio: self.apu.consume_samples(),
        //     fps: FPS,
        // })
        None
    }

    fn save(&self) -> Option<Vec<u8>> {
        println!("TODO: Save");
        //Some(self.save_state())
        None
    }
    fn load(&mut self, data: &mut Vec<u8>) {
        println!("TODO: Load");

        //self.load_state(data);
    }

    fn get_gui(&mut self) -> Option<&mut dyn GuiComponent> {
        None
    }
}
