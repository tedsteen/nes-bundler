use anyhow::Context;
use rusticnes_core::cartridge::mapper_from_file;

use super::{FrameData, LocalNesState, NesStateHandler, VideoFrame};
use crate::{
    input::JoypadState,
    settings::{Settings, MAX_PLAYERS},
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
        nes.apu
            .set_sample_rate(Settings::current().audio.sample_rate as u64);
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

static NTSC_PAL: &[u8] = include_bytes!("../../config/ntscpalette.pal");

impl NesStateHandler for RusticNesState {
    fn advance(
        &mut self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        video_frame: &mut Option<&mut VideoFrame>,
    ) -> Option<FrameData> {
        #[cfg(feature = "debug")]
        puffin::profile_function!();
        self.nes.p1_input = *joypad_state[0];
        self.nes.p2_input = *joypad_state[1];

        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("run_until_vblank");
            self.nes.run_until_vblank();
        }

        if let Some(video_frame) = video_frame {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("copy video_frame");
            self.nes
                .ppu
                .screen
                .iter()
                .enumerate()
                .for_each(|(idx, &palette_index)| {
                    let palette_index = palette_index as usize * 3;
                    let pixel_index = idx * 3;
                    video_frame[pixel_index..pixel_index + 3].clone_from_slice(
                        NTSC_PAL[palette_index..palette_index + 3]
                            .try_into()
                            .unwrap(),
                    );
                });
        }

        Some(FrameData {
            audio: self
                .nes
                .apu
                .consume_samples()
                .iter()
                .map(|&s| s as f32 / -(i16::MIN as f32))
                .collect::<Vec<f32>>(),
        })
    }

    fn save(&self) -> Option<Vec<u8>> {
        Some(self.nes.save_state())
    }
    fn load(&mut self, data: &mut Vec<u8>) {
        self.nes.load_state(data);
    }

    fn discard_samples(&mut self) {
        self.nes.apu.consume_samples();
    }
}
