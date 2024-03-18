use std::{
    io::Cursor,
    sync::{Arc, Mutex},
};

use anyhow::{bail, Result};

use tetanes_core::{
    self,
    bus::Bus,
    cart::Cart,
    common::{Clock, NesRegion, Regional, Reset, ResetKind},
    cpu::Cpu,
    input::{Joypad, Player},
};

use self::resampling::Resampler;

use super::{FrameData, NesStateHandler, NTSC_PAL};
use crate::{
    input::JoypadInput,
    settings::{gui::GuiComponent, MAX_PLAYERS},
};

mod resampling;

#[derive(Clone)]
pub struct TetanesNesState {
    cpu: Cpu,
    resampler: Arc<Mutex<Resampler>>,
}

impl TetanesNesState {
    pub fn load_rom(rom: &[u8]) -> Self {
        let mut cpu = Cpu::new(Bus::new(tetanes_core::mem::RamState::Random));
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

        Self {
            resampler: Arc::new(Mutex::new(Resampler::new(cpu.clock_rate() as u64, 44100))),
            cpu,
        }
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
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Option<FrameData> {
        *self.cpu.bus.input.joypad_mut(Player::One) = Joypad::signature((*inputs[0]).into());
        *self.cpu.bus.input.joypad_mut(Player::Two) = Joypad::signature((*inputs[1]).into());

        self.cpu.bus.clear_audio_samples();

        self.clock_frame().expect("Failed to clock the NES");

        let audio = self.cpu.bus.audio_samples();
        let resampled_audio = self.resampler.lock().unwrap().process(audio);

        let video = self
            .cpu
            .bus
            .ppu
            .frame_buffer()
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
            audio: resampled_audio,
            fps: crate::FPS,
        })
    }

    fn save(&self) -> Option<Vec<u8>> {
        Some(bincode::serialize(&self.cpu).expect("Could not save state"))
    }
    fn load(&mut self, data: &mut Vec<u8>) {
        self.cpu = bincode::deserialize(data).expect("Could not load state");
    }

    fn get_gui(&mut self) -> Option<&mut dyn GuiComponent> {
        None
    }

    fn discard_samples(&mut self) {
        self.cpu.bus.clear_audio_samples();
    }
}
