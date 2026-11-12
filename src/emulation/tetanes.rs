use std::{fmt::Debug, io::Cursor};

use anyhow::Result;

use tetanes_core::{
    common::{NesRegion, ResetKind},
    control_deck::{Clocked, Config, ControlDeck},
    input::{JoypadBtnState, Player},
    memory::RamState,
};

use super::{DEFAULT_SAMPLE_RATE, NESBuffers, NTSC_PAL, NesStateHandler};
use crate::{
    bundle::Bundle,
    input::JoypadState,
    settings::{MAX_PLAYERS, Settings},
};

#[derive(Clone)]
pub struct TetanesNesState {
    deck: ControlDeck,
}
trait ToTetanesRegion {
    fn to_tetanes_region(&self) -> NesRegion;
}

impl ToTetanesRegion for crate::emulation::NesRegion {
    fn to_tetanes_region(&self) -> NesRegion {
        match self {
            crate::emulation::NesRegion::Pal => NesRegion::Pal,
            crate::emulation::NesRegion::Ntsc => NesRegion::Ntsc,
            crate::emulation::NesRegion::Dendy => NesRegion::Dendy,
        }
    }
}

impl TetanesNesState {
    pub fn start_rom(
        rom: &[u8],
        load_sram: bool,
        region: &crate::emulation::NesRegion,
    ) -> Result<Self> {
        let region = region.to_tetanes_region();

        log::debug!("Starting ROM with system region {region:?}");
        let mut deck = ControlDeck::with_config(
            Config::default()
                .with_region(region)
                .with_ram_state(RamState::Random)
                // We keep battery-backed RAM in our own settings, so the deck does no file access.
                .with_sram_dir(None)
                // Hides a frame of input lag. The deck snapshots, clocks ahead and rewinds itself.
                .with_run_ahead(1),
        );
        deck.load_rom(Bundle::current().config.name.clone(), &mut Cursor::new(rom))?;

        // A no-op for carts without a battery.
        if load_sram && let Some(b64_encoded_sram) = &Settings::current().save_state {
            use base64::Engine;
            use base64::engine::general_purpose::STANDARD_NO_PAD as b64;
            match b64.decode(b64_encoded_sram) {
                Ok(sram) => {
                    log::info!("Loading SRAM save state");
                    deck.set_sram(&sram);
                }
                Err(err) => {
                    log::warn!("Failed to base64 decode sram: {err:?}");
                }
            }
        }

        let mut s = Self { deck };
        s.set_speed(1.0); // Triggers the correct sample rate
        Ok(s)
    }

    fn clock_frame(&mut self) {
        #[cfg(feature = "debug")]
        puffin::profile_function!();

        while self.deck.clock_frame().expect("NES to clock a frame") == Clocked::Continue {}
    }

    async fn clock_frame_into(&mut self, mut buffers: Option<NESBuffers<'_>>) {
        #[cfg(feature = "debug")]
        puffin::profile_function!();

        self.clock_frame();

        if let Some(buffers) = buffers.take() {
            if let Some(video) = buffers.video {
                #[cfg(feature = "debug")]
                puffin::profile_scope!("copy buffers");
                self.deck.frame_buffer_raw().iter().enumerate().for_each(
                    |(idx, &palette_index)| {
                        let palette_index = palette_index as usize * 3;
                        let pixel_index = idx * 4;
                        video[pixel_index..pixel_index + 3]
                            .clone_from_slice(&NTSC_PAL[palette_index..palette_index + 3]);
                    },
                );
            }
            {
                #[cfg(feature = "debug")]
                puffin::profile_scope!("Pushing audio");

                buffers.audio.push_all(self.deck.audio_samples()).await;
            }
        }
    }

    fn reset(&mut self, hard: bool) {
        let kind = if hard {
            ResetKind::Hard
        } else {
            ResetKind::Soft
        };
        self.deck.reset(kind);
    }

    fn set_region(&mut self, region: NesRegion) {
        self.deck.set_region(region);
    }
}

impl NesStateHandler for TetanesNesState {
    fn set_speed(&mut self, speed: f32) {
        let speed = speed.max(0.005);
        let target_sample_rate = match self.deck.region() {
            // Downsample a tiny bit extra to match the most common screen refresh rate (60hz)
            NesRegion::Ntsc => {
                DEFAULT_SAMPLE_RATE * (crate::emulation::NesRegion::Ntsc.to_fps() / 60.0)
            }
            _ => DEFAULT_SAMPLE_RATE,
        };

        let new_sample_rate = target_sample_rate * (1.0 / speed);

        // Guarded because setting it rebuilds the filter chain and the synth, and netplay adjusts
        // speed every frame.
        if self.deck.bus().apu.sample_rate != new_sample_rate {
            log::trace!("Change emulation speed to {speed}x");
            self.deck.set_sample_rate(new_sample_rate);
        }
    }

    async fn advance(
        &mut self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        buffers: Option<NESBuffers<'_>>,
    ) {
        self.deck.joypad_mut(Player::One).buttons =
            JoypadBtnState::from_bits_truncate(*joypad_state[0] as u16);
        self.deck.joypad_mut(Player::Two).buttons =
            JoypadBtnState::from_bits_truncate(*joypad_state[1] as u16);

        self.clock_frame_into(buffers).await;
    }

    fn save_sram(&mut self) -> Option<&[u8]> {
        // Empty for carts without a battery.
        Some(self.deck.sram()).filter(|sram| !sram.is_empty())
    }

    fn frame(&self) -> u32 {
        self.deck.frame_number()
    }

    fn reset(&mut self, hard: bool) {
        //Set the region in case it has been changed since last start/reset
        self.set_region(Settings::current_mut().nes_region_mut().to_tetanes_region());
        self.reset(hard);
    }
}

impl Debug for TetanesNesState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TetanesNesState")
            .field("frame", &self.frame())
            .finish()
    }
}
