use anyhow::{Context, Result};
use rusticnes_core::cartridge::mapper_from_file;

use crate::{
    input::JoypadInput,
    settings::{gui::GuiComponent, MAX_PLAYERS},
    Fps,
};
use std::ops::{Deref, DerefMut};
pub mod local;

pub struct NesState(pub rusticnes_core::nes::NesState, pub bool);
impl Deref for NesState {
    type Target = rusticnes_core::nes::NesState;
    fn deref(&self) -> &rusticnes_core::nes::NesState {
        &self.0
    }
}

impl DerefMut for NesState {
    fn deref_mut(&mut self) -> &mut rusticnes_core::nes::NesState {
        &mut self.0
    }
}

impl Clone for NesState {
    fn clone(&self) -> Self {
        let data = &mut self.save();
        let mut clone = Self(
            rusticnes_core::nes::NesState::new(self.0.mapper.clone()),
            false,
        );
        clone.load(data);
        clone
    }
}

pub trait NesStateHandler: GuiComponent {
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Fps;
    fn consume_samples(&mut self) -> Vec<i16>;
    fn get_frame(&self) -> Option<Vec<u16>>;
    fn save(&self) -> Vec<u8>;
    fn load(&mut self, data: &mut Vec<u8>);
    fn get_gui(&mut self) -> &mut dyn GuiComponent;
}

pub fn start_nes(cart_data: Vec<u8>, sample_rate: u64) -> Result<NesState> {
    let rom_data = match std::env::var("ROM_FILE") {
        Ok(rom_file) => {
            std::fs::read(&rom_file).context(format!("Could not read ROM {}", rom_file))?
        }
        Err(_e) => cart_data.to_vec(),
    };

    let mapper = mapper_from_file(rom_data.as_slice())
        .map_err(anyhow::Error::msg)
        .context("Failed to load ROM")?;
    #[cfg(feature = "debug")]
    mapper.print_debug_status();
    let mut nes = NesState(rusticnes_core::nes::NesState::new(mapper), false);
    nes.power_on();
    nes.apu.set_sample_rate(sample_rate);

    Ok(nes)
}
