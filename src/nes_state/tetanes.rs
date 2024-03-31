use std::io::Cursor;

use anyhow::Result;

use tetanes_core::{
    apu::filter::FilterChain,
    common::{NesRegion, Regional},
    control_deck::{Config, ControlDeck},
    cpu::Cpu,
    input::{FourPlayer, Joypad, Player},
    mem::RamState,
    video::VideoFilter,
};

use super::{
    emulator::{Emulator, SAMPLE_RATE},
    FrameData, NesStateHandler, NTSC_PAL,
};
use crate::{bundle::Bundle, input::JoypadState, settings::MAX_PLAYERS, window::NESFrame};

#[derive(Clone)]
pub struct TetanesNesState {
    control_deck: ControlDeck,
}

pub trait ToTetanesRegion {
    fn to_tetanes_region(&self) -> NesRegion;
}

impl ToTetanesRegion for crate::bundle::NesRegion {
    fn to_tetanes_region(&self) -> NesRegion {
        match self {
            crate::bundle::NesRegion::Pal => NesRegion::Pal,
            crate::bundle::NesRegion::Ntsc => NesRegion::Ntsc,
            crate::bundle::NesRegion::Dendy => NesRegion::Dendy,
        }
    }
}

impl TetanesNesState {
    pub fn start_rom(rom: &[u8]) -> Result<Self> {
        let region = Bundle::current().config.nes_region.to_tetanes_region();
        let config = Config {
            filter: VideoFilter::Pixellate,
            region,
            ram_state: RamState::Random,
            four_player: FourPlayer::Disabled,
            zapper: false,
            genie_codes: vec![],
            concurrent_dpad: false,
            channels_enabled: [true; 5],
        };
        log::debug!("Starting ROM with configuration {config:?}");
        let mut control_deck = ControlDeck::with_config(config);
        //control_deck.set_cycle_accurate(false); //TODO: Add as a bundle config?
        control_deck.load_rom(Bundle::current().config.name.clone(), &mut Cursor::new(rom))?;

        control_deck.set_region(region);

        Ok(Self { control_deck })
    }

    fn set_speed(&mut self, speed: f32) {
        let apu = &mut self.control_deck.cpu_mut().bus.apu;
        let new_sample_rate = SAMPLE_RATE * (1.0 / speed);
        let new_sample_period = Cpu::region_clock_rate(apu.region) / new_sample_rate;

        if apu.sample_period != new_sample_period {
            log::debug!("Change emulation speed to {speed}x");
            apu.filter_chain = FilterChain::new(apu.region, new_sample_rate);
            apu.sample_period = new_sample_period;
        }
    }
}

impl NesStateHandler for TetanesNesState {
    fn advance(
        &mut self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        nes_frame: &mut Option<&mut NESFrame>,
    ) -> Option<FrameData> {
        self.set_speed(*Emulator::emulation_speed().lock().unwrap());

        *self.control_deck.joypad_mut(Player::One) = Joypad::from_bytes((*joypad_state[0]).into());
        *self.control_deck.joypad_mut(Player::Two) = Joypad::from_bytes((*joypad_state[1]).into());

        self.control_deck.clear_audio_samples();

        self.control_deck
            .clock_frame()
            .expect("NES to clock a frame");

        let audio = self.control_deck.audio_samples();

        if let Some(nes_frame) = nes_frame {
            self.control_deck
                .cpu()
                .bus
                .ppu
                .frame_buffer()
                .iter()
                .enumerate()
                .for_each(|(idx, &palette_index)| {
                    let palette_index = palette_index as usize * 3;
                    let pixel_index = idx * 4;
                    nes_frame[pixel_index..pixel_index + 3]
                        .clone_from_slice(&NTSC_PAL[palette_index..palette_index + 3]);
                });
        }
        Some(FrameData {
            audio: audio.to_vec(),
        })
    }

    fn save(&self) -> Option<Vec<u8>> {
        Some(bincode::serialize(&self.control_deck.cpu()).expect("NES state to serialize"))
    }
    fn load(&mut self, data: &mut Vec<u8>) {
        *self.control_deck.cpu_mut() =
            bincode::deserialize(data).expect("NES state to deserialize");
    }

    fn discard_samples(&mut self) {
        self.control_deck.clear_audio_samples();
    }

    fn frame(&self) -> u32 {
        self.control_deck.frame_number()
    }
}
