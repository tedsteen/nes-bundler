use std::{fmt::Debug, io::Cursor};

use anyhow::Result;

use tetanes_core::{
    common::{NesRegion, ResetKind},
    control_deck::{Clocked, Config, ControlDeck},
    input::{JoypadBtnState, Player},
    memory::RamState,
};

use super::{DEFAULT_SAMPLE_RATE, NESBuffers, NesStateHandler};
use crate::{
    bundle::Bundle,
    input::JoypadState,
    settings::{MAX_PLAYERS, Settings},
};

#[derive(Clone)]
pub struct TetanesNesState {
    deck: ControlDeck,
    /// The speed last given to the deck, in tetanes' quarter steps.
    frame_speed: f32,
    /// Netplay slows the frame clock down or speeds it up to meet the other player; the audio has
    /// to stretch by as much to keep up.
    catch_up_speed: f32,
    region: crate::emulation::NesRegion,
    /// The last frame failed, so the error is logged once rather than every frame.
    failed: bool,
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
    /// Frames to run ahead of the console to hide a frame of input lag.
    const RUN_AHEAD: usize = 1;

    pub fn start_rom(
        rom: &[u8],
        load_sram: bool,
        region_setting: &crate::emulation::NesRegion,
    ) -> Result<Self> {
        let region = region_setting.to_tetanes_region();

        log::debug!("Starting ROM with system region {region:?}");
        let mut deck = ControlDeck::with_config(
            Config::default()
                .with_region(region)
                .with_ram_state(RamState::Random)
                // We keep battery-backed RAM in our own settings, so the deck does no file access.
                .with_sram_dir(None)
                // Hides a frame of input lag. The deck snapshots, clocks ahead and rewinds itself.
                .with_run_ahead(Self::RUN_AHEAD),
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

        deck.set_sample_rate(DEFAULT_SAMPLE_RATE);
        Ok(Self {
            deck,
            frame_speed: 1.0,
            catch_up_speed: 1.0,
            region: region_setting.clone(),
            failed: false,
        })
    }

    fn clock_frame(&mut self) {
        #[cfg(feature = "debug")]
        puffin::profile_function!();

        loop {
            match self.deck.clock_frame() {
                Ok(Clocked::Continue) => {}
                Ok(_) => {
                    self.failed = false;
                    break;
                }
                // The game crashed the console (a jammed CPU, say). Keep the app up; a reset
                // gets the game going again.
                Err(e) => {
                    if !std::mem::replace(&mut self.failed, true) {
                        log::error!("The emulated NES failed: {e}");
                    }
                    break;
                }
            }
        }
    }

    fn clock_frame_into(&mut self, mut buffers: Option<NESBuffers<'_>>) {
        #[cfg(feature = "debug")]
        puffin::profile_function!();

        if let Some(buffers) = &mut buffers {
            // Keeps the audio buffer level while the display sets the pace (dynamic rate
            // control), and stretches the audio as much as netplay slows the frames down.
            self.deck
                .set_audio_sample_ratio(buffers.audio.rate_control() / self.catch_up_speed);
        }
        self.clock_frame();

        if let Some(buffers) = buffers.take() {
            if let Some(video) = buffers.video {
                video.copy_from_slice(self.deck.frame_buffer_raw());
            }
            {
                #[cfg(feature = "debug")]
                puffin::profile_scope!("Pushing audio");

                buffers.audio.push(self.deck.audio_samples());
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

    /// Runs `speed` NES frames per display frame, in tetanes' quarter steps (0.25x to 2x). Tetanes
    /// resamples the audio to match, so the display keeps setting the pace.
    fn set_frame_speed(&mut self, speed: f32) {
        let speed = ((speed * 4.0).round() / 4.0).clamp(0.25, 2.0);
        // Guarded because setting it rebuilds the APU's filter chain.
        if speed != self.frame_speed {
            log::debug!("Emulation speed {speed}x");
            self.frame_speed = speed;
            self.deck.set_frame_speed(speed);
        }
    }

    /// How much slower or faster than the display netplay runs, to meet the other player. The
    /// frame clock changes by this much (see `NesStateHandler::pace`); this stretches the audio to
    /// match.
    #[cfg(feature = "netplay")]
    pub fn set_catch_up_speed(&mut self, speed: f32) {
        self.catch_up_speed = speed;
    }
}

impl NesStateHandler for TetanesNesState {
    async fn advance(
        &mut self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        buffers: Option<NESBuffers<'_>>,
        speed: f32,
    ) {
        self.deck.joypad_mut(Player::One).buttons =
            JoypadBtnState::from_bits_truncate(*joypad_state[0] as u16);
        self.deck.joypad_mut(Player::Two).buttons =
            JoypadBtnState::from_bits_truncate(*joypad_state[1] as u16);

        self.set_frame_speed(speed);
        // Frames clocked without buffers are netplay rollback replays, which are never shown, so
        // running ahead of them (to hide input lag) would only double their cost. It doesn't
        // change the emulation: the deck rewinds to the real frame after running ahead.
        self.deck.set_run_ahead(if buffers.is_some() {
            Self::RUN_AHEAD
        } else {
            0
        });
        self.clock_frame_into(buffers);
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
        self.region = Settings::current().nes_region();
        self.set_region(self.region.to_tetanes_region());
        self.reset(hard);
    }

    fn region(&self) -> crate::emulation::NesRegion {
        self.region.clone()
    }
}

impl Debug for TetanesNesState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TetanesNesState")
            .field("frame", &self.frame())
            .finish()
    }
}
