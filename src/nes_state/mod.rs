use anyhow::{Context, Result};
use rusticnes_core::{cartridge::mapper_from_file, mmc::mapper::Mapper};

use crate::{
    bundle::Bundle,
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

pub fn get_mapper(bundle: &Bundle) -> Result<Box<dyn Mapper>, anyhow::Error> {
    let rom_data = match std::env::var("ROM_FILE") {
        Ok(rom_file) => {
            std::fs::read(&rom_file).context(format!("Could not read ROM {}", rom_file))?
        }
        Err(_e) => bundle.rom.to_vec(),
    };
    mapper_from_file(&rom_data)
        .map_err(anyhow::Error::msg)
        .context("Failed to load ROM")
}
