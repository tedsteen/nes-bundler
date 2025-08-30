use std::{io::Cursor, ops::Deref};

use anyhow::Result;

use ringbuf::traits::Producer;
use tetanes_core::{
    apu::filter::FilterChain,
    common::{NesRegion, Regional, Reset, ResetKind},
    control_deck::{Config, ControlDeck, HeadlessMode, MapperRevisionsConfig},
    cpu::Cpu,
    fs,
    input::{FourPlayer, Joypad, Player},
    mem::RamState,
    video::VideoFilter,
};

use super::{NESBuffers, NTSC_PAL, NesStateHandler, SAMPLE_RATE};
use crate::{
    bundle::Bundle,
    input::JoypadState,
    settings::{MAX_PLAYERS, Settings},
};

#[derive(Clone)]
pub struct TetanesNesState {
    control_deck: ControlDeck,
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
        let config = Config {
            filter: VideoFilter::Pixellate,
            region,
            ram_state: RamState::Random,
            four_player: FourPlayer::Disabled,
            zapper: false,
            genie_codes: vec![],
            concurrent_dpad: false,
            channels_enabled: [true; 6],
            headless_mode: HeadlessMode::empty(),
            cycle_accurate: false,
            data_dir: Config::default_data_dir(),
            mapper_revisions: MapperRevisionsConfig::default(),
            emulate_ppu_warmup: false,
        };
        log::debug!("Starting ROM with configuration {config:?}");
        let mut control_deck = ControlDeck::with_config(config);
        //control_deck.set_cycle_accurate(false); //TODO: Add as a bundle config?
        control_deck.load_rom(Bundle::current().config.name.clone(), &mut Cursor::new(rom))?;

        if load_sram {
            if let Some(true) = control_deck.cart_battery_backed() {
                if let Some(b64_encoded_sram) = &Settings::current().save_state {
                    use base64::Engine;
                    use base64::engine::general_purpose::STANDARD_NO_PAD as b64;
                    match b64.decode(b64_encoded_sram) {
                        Ok(sram) => {
                            log::info!("Loading SRAM save state");
                            control_deck.cpu_mut().bus.load_sram(sram);
                        }
                        Err(err) => {
                            log::warn!("Failed to base64 decode sram: {err:?}");
                        }
                    }
                }
            }
        }

        control_deck.set_region(region);
        let mut s = Self { control_deck };
        s.set_speed(1.0); // Trigger the correct sample rate
        Ok(s)
    }

    pub fn clock_frame_into(&mut self, buffers: &mut NESBuffers) -> Result<u64> {
        #[cfg(feature = "debug")]
        puffin::profile_function!();

        self.control_deck.cpu_mut().bus.ppu.skip_rendering = false;
        //self.control_deck.cpu_mut().bus.apu.skip_mixing = false;

        let cycles = self.control_deck.clock_frame()?;
        if let Some(video) = &mut buffers.video {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("copy buffers");
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
                    video[pixel_index..pixel_index + 3]
                        .clone_from_slice(&NTSC_PAL[palette_index..palette_index + 3]);
                });
        }
        if let Some(audio_producer) = &mut buffers.audio {
            let samples = self.control_deck.cpu().bus.audio_samples();
            audio_producer.push_slice(samples);
        }

        self.control_deck.clear_audio_samples();
        Ok(cycles)
    }

    pub fn clock_frame_ahead_into(&mut self, buffers: &mut NESBuffers) -> Result<u64> {
        #[cfg(feature = "debug")]
        puffin::profile_function!();

        self.control_deck.cpu_mut().bus.ppu.skip_rendering = true;
        //self.control_deck.cpu_mut().bus.apu.skip_mixing = true;
        // Clock current frame and discard video
        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("clock frame");
            self.control_deck.clock_frame()?;
        }

        // Save state so we can rewind
        let state = {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("serialize");
            postcard::to_allocvec(self.control_deck.cpu())
                .map_err(|err| fs::Error::SerializationFailed(err.to_string()))?
        };

        // Discard audio and only output the future frame/audio
        self.control_deck.clear_audio_samples();
        let cycles = self.clock_frame_into(buffers)?;

        // Restore back to current frame
        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("deserialize and load");
            let state = postcard::from_bytes(state.deref())
                .map_err(|err| fs::Error::DeserializationFailed(err.to_string()))?;
            self.control_deck.load_cpu(state);
        }

        Ok(cycles)
    }
}

impl NesStateHandler for TetanesNesState {
    fn set_speed(&mut self, speed: f32) {
        let speed = speed.max(0.005);
        let apu = &mut self.control_deck.cpu_mut().bus.apu;
        let target_sample_rate = match apu.region {
            // Downsample a tiny bit extra to match the most common screen refresh rate (60hz)
            NesRegion::Ntsc => SAMPLE_RATE * (crate::emulation::NesRegion::Ntsc.to_fps() / 60.0),
            _ => SAMPLE_RATE,
        };

        let new_sample_rate = target_sample_rate * (1.0 / speed);
        let new_sample_period = Cpu::region_clock_rate(apu.region) / new_sample_rate;

        if apu.sample_period != new_sample_period {
            log::debug!("Change emulation speed to {speed}x");
            apu.filter_chain = FilterChain::new(apu.region, new_sample_rate);
            apu.sample_period = new_sample_period;
        }
    }

    fn advance(&mut self, joypad_state: [JoypadState; MAX_PLAYERS], buffers: &mut NESBuffers) {
        *self.control_deck.joypad_mut(Player::One) = Joypad::from_bytes((*joypad_state[0]).into());
        *self.control_deck.joypad_mut(Player::Two) = Joypad::from_bytes((*joypad_state[1]).into());

        self.clock_frame_ahead_into(buffers)
            .expect("NES to clock a frame");
    }

    fn save_sram(&self) -> Option<&[u8]> {
        if let Some(true) = self.control_deck.cart_battery_backed() {
            Some(self.control_deck.sram())
        } else {
            None
        }
    }

    #[cfg(feature = "debug")]
    fn frame(&self) -> u32 {
        self.control_deck.frame_number()
    }

    fn reset(&mut self, hard: bool) {
        let kind = if hard {
            ResetKind::Hard
        } else {
            ResetKind::Soft
        };
        //Set the region in case it has been changed since last start/reset
        self.control_deck
            .set_region(Settings::current_mut().get_nes_region().to_tetanes_region());
        self.control_deck.reset(kind);
    }
}
