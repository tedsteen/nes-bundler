use anyhow::Context;
use rusticnes_core::cartridge::mapper_from_file;

use super::{FrameData, LocalNesState, NesState, NesStateHandler};
use crate::{
    input::JoypadInput,
    settings::{gui::GuiComponent, MAX_PLAYERS},
    FPS,
};
use std::ops::{Deref, DerefMut};

impl LocalNesState {
    pub fn load_rom(rom: &[u8]) -> LocalNesState {
        let mapper = mapper_from_file(rom)
            .map_err(anyhow::Error::msg)
            .context("Failed to load ROM")
            .unwrap();
        let mut nes = NesState(rusticnes_core::nes::NesState::new(mapper));
        nes.power_on();
        nes
    }
}

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
        let mut clone = Self(rusticnes_core::nes::NesState::new(self.mapper.clone()));
        if let Some(data) = &mut self.save() {
            clone.load(data);
        }
        clone
    }
}

impl NesStateHandler for LocalNesState {
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Option<FrameData> {
        self.p1_input = *inputs[0];
        self.p2_input = *inputs[1];
        self.run_until_vblank();
        Some(FrameData {
            video: self.ppu.screen.clone(),
            audio: self.apu.consume_samples(),
            fps: FPS,
        })
    }

    fn save(&self) -> Option<Vec<u8>> {
        Some(self.save_state())
    }
    fn load(&mut self, data: &mut Vec<u8>) {
        self.load_state(data);
    }

    fn get_gui(&mut self) -> Option<&mut dyn GuiComponent> {
        None
    }
}
