use std::io::Cursor;

use anyhow::{bail, Result};

use tetanes_core::{
    self,
    bus::Bus,
    cart::Cart,
    common::{Clock, NesRegion, Regional, Reset, ResetKind},
    cpu::Cpu,
    input::{Joypad, Player},
};

use super::{FrameData, NesStateHandler, NTSC_PAL};
use crate::{
    input::JoypadState,
    settings::{Settings, MAX_PLAYERS},
    window::NESFrame,
};

#[derive(Clone)]
pub struct TetanesNesState {
    cpu: Cpu,
    speed: f32,
}

impl TetanesNesState {
    pub fn load_rom(rom: &[u8]) -> Self {
        let mut cpu = Cpu::new(Bus::new(
            tetanes_core::mem::RamState::Random,
            Settings::current().audio.sample_rate as f32,
        ));
        cpu.set_region(NesRegion::default());
        cpu.bus
            .input
            .set_four_player(tetanes_core::input::FourPlayer::Disabled);
        cpu.bus.input.connect_zapper(false);

        let cart = Cart::from_rom("Name", &mut Cursor::new(rom), cpu.bus.ram_state)
            .expect("Could not load cart");
        cpu.set_region(cart.region());
        cpu.bus.load_cart(cart);
        cpu.reset(ResetKind::Hard);

        Self { cpu, speed: 1.0 }
    }

    pub const fn frame_number(&self) -> u32 {
        self.cpu.bus.ppu.frame_number()
    }
    pub fn clock_instr(&mut self) -> Result<usize> {
        let cycles = self.cpu.clock();
        if self.cpu.corrupted {
            bail!("cpu corrupted")
        }
        Ok(cycles)
    }

    pub fn clock_frame(&mut self) -> Result<usize> {
        let mut total_cycles = 0;
        let frame = self.frame_number();
        while frame == self.frame_number() {
            total_cycles += self.clock_instr()?;
        }
        Ok(total_cycles)
    }
}

impl NesStateHandler for TetanesNesState {
    fn advance(
        &mut self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        nes_frame: &mut Option<&mut NESFrame>,
    ) -> Option<FrameData> {
        let input = &mut self.cpu.bus.input;
        *input.joypad_mut(Player::One) = Joypad::signature((*joypad_state[0]).into());
        *input.joypad_mut(Player::Two) = Joypad::signature((*joypad_state[1]).into());

        self.cpu.bus.clear_audio_samples();

        self.clock_frame().expect("Failed to clock the NES");

        let audio = self.cpu.bus.audio_samples();

        if let Some(nes_frame) = nes_frame {
            self.cpu
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
        Some(bincode::serialize(&self.cpu).expect("Could not save state"))
    }
    fn load(&mut self, data: &mut Vec<u8>) {
        self.cpu = bincode::deserialize(data).expect("Could not load state");
    }

    fn discard_samples(&mut self) {
        self.cpu.bus.clear_audio_samples();
    }

    fn set_speed(&mut self, speed: f32) {
        let speed = speed.max(0.01);
        if self.speed != speed {
            log::debug!("Setting emulation speed: {speed}");
            self.cpu
                .bus
                .apu
                .set_sample_rate(Settings::current().audio.sample_rate as f32 * (1.0 / speed));
            self.speed = speed;
        }
    }
}
