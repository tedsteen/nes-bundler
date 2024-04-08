use std::io::Cursor;

use anyhow::Result;

use tetanes_core::{
    apu::filter::FilterChain,
    common::{NesRegion, Regional},
    control_deck::{Config, ControlDeck, HeadlessMode},
    cpu::Cpu,
    fs,
    input::{FourPlayer, Joypad, Player},
    mem::RamState,
    video::VideoFilter,
};

use super::{Emulator, NESBuffers, NesStateHandler, NTSC_PAL, SAMPLE_RATE};
use crate::{
    bundle::Bundle,
    input::JoypadState,
    settings::{Settings, MAX_PLAYERS},
};

#[derive(Clone)]
pub struct TetanesNesState {
    control_deck: ControlDeck,
}

trait ToTetanesRegion {
    fn to_tetanes_region(&self) -> NesRegion;
}
trait ToSampleRate {
    fn to_sample_rate(&self) -> f32;
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
    pub fn start_rom(rom: &[u8], load_sram: bool) -> Result<Self> {
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
            headless_mode: HeadlessMode::empty(),
        };
        log::debug!("Starting ROM with configuration {config:?}");
        let mut control_deck = ControlDeck::with_config(config);
        //control_deck.set_cycle_accurate(false); //TODO: Add as a bundle config?
        control_deck.load_rom(Bundle::current().config.name.clone(), &mut Cursor::new(rom))?;

        if load_sram {
            if let Some(true) = control_deck.cart_battery_backed() {
                if let Some(b64_encoded_sram) = &Settings::current().save_state {
                    use base64::engine::general_purpose::STANDARD_NO_PAD as b64;
                    use base64::Engine;
                    match b64.decode(b64_encoded_sram) {
                        Ok(sram) => {
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

    fn set_speed(&mut self, speed: f32) {
        let speed = speed.max(0.01).min(1.0);
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

    pub fn clock_frame_into(&mut self, buffers: Option<&mut NESBuffers>) -> Result<usize> {
        self.control_deck.cpu_mut().bus.ppu.skip_rendering = false;
        let cycles = self.control_deck.clock_frame()?;
        if let Some(buffers) = buffers {
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
                    buffers.video[pixel_index..pixel_index + 3]
                        .clone_from_slice(&NTSC_PAL[palette_index..palette_index + 3]);
                });
            buffers
                .audio
                .extend_from_slice(&self.control_deck.cpu().bus.audio_samples());
        }

        self.control_deck.clear_audio_samples();
        Ok(cycles)
    }

    pub fn clock_frame_ahead_into(&mut self, buffers: Option<&mut NESBuffers>) -> Result<usize> {
        self.control_deck.cpu_mut().bus.ppu.skip_rendering = true;
        // Clock current frame and discard video
        self.control_deck.clock_frame()?;
        // Save state so we can rewind
        let state = bincode::serialize(self.control_deck.cpu())
            .map_err(|err| fs::Error::SerializationFailed(err.to_string()))?;

        // Discard audio and only output the future frame/audio
        self.control_deck.clear_audio_samples();
        let cycles = self.clock_frame_into(buffers)?;

        // Restore back to current frame
        let state = bincode::deserialize(&state)
            .map_err(|err| fs::Error::DeserializationFailed(err.to_string()))?;
        self.control_deck.load_cpu(state);

        Ok(cycles)
    }
}

impl NesStateHandler for TetanesNesState {
    fn advance(
        &mut self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        buffers: Option<&mut NESBuffers>,
    ) {
        #[cfg(feature = "debug")]
        puffin::profile_function!();

        self.set_speed(*Emulator::emulation_speed());

        *self.control_deck.joypad_mut(Player::One) = Joypad::from_bytes((*joypad_state[0]).into());
        *self.control_deck.joypad_mut(Player::Two) = Joypad::from_bytes((*joypad_state[1]).into());

        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("clock frame");

            self.clock_frame_ahead_into(buffers)
                .expect("NES to clock a frame");
        }
    }

    fn save_sram(&self) -> Option<Vec<u8>> {
        if let Some(true) = self.control_deck.cart_battery_backed() {
            Some(bincode::serialize(&self.control_deck.cpu()).expect("NES state to serialize"))
        } else {
            None
        }
    }
    fn load_sram(&mut self, data: &mut Vec<u8>) {
        if let Some(true) = self.control_deck.cart_battery_backed() {
            *self.control_deck.cpu_mut() =
                bincode::deserialize(data).expect("NES state to deserialize");
        }
    }

    fn frame(&self) -> u32 {
        self.control_deck.frame_number()
    }
}
