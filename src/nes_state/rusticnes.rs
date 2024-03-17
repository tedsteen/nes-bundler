use anyhow::Context;
use rusticnes_core::cartridge::mapper_from_file;

use super::{FrameData, LocalNesState, NesStateHandler};
use crate::{
    input::JoypadInput,
    settings::{gui::GuiComponent, MAX_PLAYERS},
    FPS,
};

pub struct RusticNesState {
    nes: rusticnes_core::nes::NesState,
}

impl RusticNesState {
    pub fn load_rom(rom: &[u8]) -> LocalNesState {
        let mapper = mapper_from_file(rom)
            .map_err(anyhow::Error::msg)
            .context("Failed to load ROM")
            .unwrap();
        let mut nes = rusticnes_core::nes::NesState::new(mapper);
        nes.power_on();
        RusticNesState { nes }
    }
}

impl Clone for RusticNesState {
    fn clone(&self) -> Self {
        let mut clone = RusticNesState {
            nes: rusticnes_core::nes::NesState::new(self.nes.mapper.clone()),
        };
        if let Some(data) = &mut self.save() {
            clone.load(data);
        }
        clone
    }
}

static NTSC_PAL: &[u8] = include_bytes!("ntscpalette.pal");

impl NesStateHandler for RusticNesState {
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Option<FrameData> {
        self.nes.p1_input = *inputs[0];
        self.nes.p2_input = *inputs[1];
        self.nes.run_until_vblank();

        let video = self
            .nes
            .ppu
            .screen
            .iter()
            .flat_map(|&palette_index| {
                let palette_index = palette_index as usize * 3;
                let rgba: [u8; 3] = NTSC_PAL[palette_index..palette_index + 3]
                    .try_into()
                    .unwrap();
                rgba
            })
            .collect::<Vec<u8>>();

        Some(FrameData {
            video,
            audio: self
                .nes
                .apu
                .consume_samples()
                .iter()
                .map(|&s| s as f32 / -(i16::MIN as f32))
                .collect(),
            fps: FPS,
        })
    }

    fn save(&self) -> Option<Vec<u8>> {
        Some(self.nes.save_state())
    }
    fn load(&mut self, data: &mut Vec<u8>) {
        self.nes.load_state(data);
    }

    fn get_gui(&mut self) -> Option<&mut dyn GuiComponent> {
        None
    }

    fn discard_samples(&mut self) {
        self.nes.apu.consume_samples();
    }
}
