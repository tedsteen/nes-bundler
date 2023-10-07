use anyhow::{Context, Result};
use rusticnes_core::cartridge::mapper_from_file;

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
    fn save(&self) -> Vec<u8>;
    fn load(&mut self, data: &mut Vec<u8>);
    fn get_gui(&mut self) -> Option<&mut dyn GuiComponent>;
}

pub fn start_nes(cart_data: Vec<u8>, sample_rate: u64) -> Result<LocalNesState> {
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
    let mut nes = LocalNesState(rusticnes_core::nes::NesState::new(mapper));
    nes.power_on();
    nes.apu.set_sample_rate(sample_rate);

    Ok(nes)
}
