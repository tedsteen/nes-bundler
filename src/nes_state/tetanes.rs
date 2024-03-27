use std::io::Cursor;

use anyhow::Result;

use tetanes_core::{
    self,
    common::{NesRegion, Regional},
    control_deck::{Config, ControlDeck},
    input::{FourPlayer, Joypad, Player},
    mem::RamState,
    video::VideoFilter,
};

use super::{FrameData, NesStateHandler, NTSC_PAL};
use crate::{
    bundle::Bundle,
    input::JoypadState,
    settings::{Settings, MAX_PLAYERS},
    window::NESFrame,
};

#[derive(Clone)]
pub struct TetanesNesState {
    control_deck: ControlDeck,
    speed: f32,
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
            dir: Bundle::current().settings_path.clone(),
            filter: VideoFilter::Pixellate,
            sample_rate: Settings::current().audio.sample_rate as f32,
            region,
            ram_state: RamState::Random,
            four_player: FourPlayer::Disabled,
            zapper: false,
            genie_codes: vec![],
            load_on_start: true,
            save_on_exit: true,
            save_slot: 1,
            concurrent_dpad: false,
            channels_enabled: [true; 5],
        };

        let mut control_deck = ControlDeck::with_config(config);
        let _ =
            control_deck.load_rom(Bundle::current().config.name.clone(), &mut Cursor::new(rom))?;
        control_deck.set_region(region);
        Ok(Self {
            control_deck,
            speed: 1.0,
        })
    }
}

impl NesStateHandler for TetanesNesState {
    fn advance(
        &mut self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        nes_frame: &mut Option<&mut NESFrame>,
    ) -> Option<FrameData> {
        *self.control_deck.joypad_mut(Player::One) = Joypad::signature((*joypad_state[0]).into());
        *self.control_deck.joypad_mut(Player::Two) = Joypad::signature((*joypad_state[1]).into());

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

    fn set_speed(&mut self, speed: f32) {
        let speed = speed.max(0.01);
        if self.speed != speed {
            log::debug!("Setting emulation speed: {speed}");
            self.control_deck
                .set_sample_rate(Settings::current().audio.sample_rate as f32 * (1.0 / speed));
            self.speed = speed;
        }
    }
}
