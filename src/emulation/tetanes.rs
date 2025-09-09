use std::{
    fmt::Debug,
    io::{Cursor, Read},
    ops::Deref,
};

use anyhow::Result;

use tetanes_core::{
    apu::filter::FilterChain,
    bus::Bus,
    cart::Cart,
    common::{Clock, NesRegion, Regional, Reset, ResetKind},
    control_deck::{LoadedRom, MapperRevisionsConfig},
    cpu::Cpu,
    fs,
    input::{JoypadBtnState, Player},
    mapper::Mapper,
    mem::RamState,
};

use super::{DEFAULT_SAMPLE_RATE, NESBuffers, NTSC_PAL, NesStateHandler};
use crate::{
    bundle::Bundle,
    input::JoypadState,
    settings::{MAX_PLAYERS, Settings},
};

#[derive(Clone)]
pub struct TetanesNesState {
    cpu: Cpu,
    battery_backed: bool,
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
        let mut cpu = Cpu::new(Bus::new(region, RamState::Random));
        let loaded_rom = Self::load_rom(
            &mut cpu,
            Bundle::current().config.name.clone(),
            &mut Cursor::new(rom),
        )?;
        let battery_backed = loaded_rom.battery_backed;

        if load_sram && battery_backed {
            if let Some(b64_encoded_sram) = &Settings::current().save_state {
                use base64::Engine;
                use base64::engine::general_purpose::STANDARD_NO_PAD as b64;
                match b64.decode(b64_encoded_sram) {
                    Ok(sram) => {
                        log::info!("Loading SRAM save state");
                        cpu.bus.load_sram(sram);
                    }
                    Err(err) => {
                        log::warn!("Failed to base64 decode sram: {err:?}");
                    }
                }
            }
        }

        let mut s = Self {
            cpu,
            battery_backed,
        };
        s.set_speed(1.0); // Triggers the correct sample rate
        Ok(s)
    }

    fn load_rom<S: ToString, F: Read>(cpu: &mut Cpu, name: S, rom: &mut F) -> Result<LoadedRom> {
        let name = name.to_string();
        let cart = Cart::from_rom(&name, rom, cpu.bus.ram_state).expect("Cart to load");

        let loaded_rom = LoadedRom {
            name: name.clone(),
            battery_backed: cart.battery_backed(),
            region: cart.region(),
        };
        cpu.bus.load_cart(cart);
        Self::update_mapper_revisions(cpu, MapperRevisionsConfig::default());
        cpu.reset(ResetKind::Hard);
        Ok(loaded_rom)
    }

    fn update_mapper_revisions(cpu: &mut Cpu, mapper_revisions: MapperRevisionsConfig) {
        match &mut cpu.bus.ppu.bus.mapper {
            Mapper::Txrom(mapper) => {
                mapper.set_revision(mapper_revisions.mmc3);
            }
            Mapper::Bf909x(mapper) => {
                mapper.set_revision(mapper_revisions.bf909);
            }
            // Remaining mappers all have more concrete detection via ROM headers
            _ => (),
        }
    }

    fn clock_frame(&mut self) {
        let frame_number = self.cpu.bus.ppu.frame_number();

        while self.cpu.bus.ppu.frame_number() == frame_number {
            self.cpu.clock();
        }
        self.cpu.bus.apu.clock_flush();
    }

    fn clear_audio_samples(&mut self) {
        self.cpu.bus.clear_audio_samples();
    }

    pub async fn clock_frame_into(&mut self, mut buffers: Option<NESBuffers<'_>>) {
        #[cfg(feature = "debug")]
        puffin::profile_function!();

        self.cpu.bus.ppu.skip_rendering = false;
        //self.control_deck.cpu_mut().bus.apu.skip_mixing = false;

        self.clock_frame();

        if let Some(buffers) = buffers.take() {
            if let Some(video) = buffers.video {
                #[cfg(feature = "debug")]
                puffin::profile_scope!("copy buffers");
                self.cpu.bus.ppu.frame_buffer().iter().enumerate().for_each(
                    |(idx, &palette_index)| {
                        let palette_index = palette_index as usize * 3;
                        let pixel_index = idx * 4;
                        video[pixel_index..pixel_index + 3]
                            .clone_from_slice(&NTSC_PAL[palette_index..palette_index + 3]);
                    },
                );
            }

            buffers.audio.push_all(self.cpu.bus.audio_samples()).await;
            self.clear_audio_samples();
        }
    }

    pub async fn clock_frame_ahead_into(&mut self, buffers: Option<NESBuffers<'_>>) -> Result<()> {
        #[cfg(feature = "debug")]
        puffin::profile_function!();

        self.cpu.bus.ppu.skip_rendering = true;
        //self.control_deck.cpu_mut().bus.apu.skip_mixing = true;
        // Clock current frame and discard video
        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("clock frame");
            self.clock_frame();
        }

        // Save state so we can rewind
        let state = {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("serialize");
            postcard::to_allocvec(&self.cpu)
                .map_err(|err| fs::Error::SerializationFailed(err.to_string()))?
        };

        // Discard audio and only output the future frame/audio
        self.clear_audio_samples();
        self.clock_frame_into(buffers).await;

        // Restore back to current frame
        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("deserialize and load");
            let state = postcard::from_bytes(state.deref())
                .map_err(|err| fs::Error::DeserializationFailed(err.to_string()))?;
            self.cpu.load(state);
        }

        Ok(())
    }

    fn sram(&self) -> &[u8] {
        self.cpu.bus.sram()
    }

    fn reset(&mut self, hard: bool) {
        let kind = if hard {
            ResetKind::Hard
        } else {
            ResetKind::Soft
        };
        self.cpu.reset(kind);
    }

    fn set_region(&mut self, region: NesRegion) {
        self.cpu.set_region(region);
    }
}

impl NesStateHandler for TetanesNesState {
    fn set_speed(&mut self, speed: f32) {
        let speed = speed.max(0.005);
        let apu = &mut self.cpu.bus.apu;
        let target_sample_rate = match apu.region {
            // Downsample a tiny bit extra to match the most common screen refresh rate (60hz)
            NesRegion::Ntsc => {
                DEFAULT_SAMPLE_RATE * (crate::emulation::NesRegion::Ntsc.to_fps() / 60.0)
            }
            _ => DEFAULT_SAMPLE_RATE,
        };

        let new_sample_rate = target_sample_rate * (1.0 / speed);
        let new_sample_period = Cpu::region_clock_rate(apu.region) / new_sample_rate;

        if apu.sample_period != new_sample_period {
            log::trace!("Change emulation speed to {speed}x");
            apu.filter_chain = FilterChain::new(apu.region, new_sample_rate);
            apu.sample_period = new_sample_period;
        }
    }

    async fn advance(
        &mut self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        buffers: Option<NESBuffers<'_>>,
    ) {
        self.cpu.bus.input.joypad_mut(Player::One).buttons =
            JoypadBtnState::from_bits_truncate(*joypad_state[0] as u16);
        self.cpu.bus.input.joypad_mut(Player::Two).buttons =
            JoypadBtnState::from_bits_truncate(*joypad_state[1] as u16);

        self.clock_frame_ahead_into(buffers)
            .await
            .expect("NES to clock a frame");
    }

    fn save_sram(&self) -> Option<&[u8]> {
        if self.battery_backed {
            Some(self.sram())
        } else {
            None
        }
    }

    fn frame(&self) -> u32 {
        self.cpu.bus.ppu.frame_number()
    }

    fn reset(&mut self, hard: bool) {
        //Set the region in case it has been changed since last start/reset
        self.set_region(Settings::current_mut().get_nes_region().to_tetanes_region());
        self.reset(hard);
    }
}

impl Debug for TetanesNesState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TetanesNesState")
            .field("frame", &self.frame())
            .field("battery_backed", &self.battery_backed)
            .finish()
    }
}
