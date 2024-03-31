use std::ops::{Deref, DerefMut};

use crate::{input::JoypadState, settings::MAX_PLAYERS, NES_HEIGHT, NES_WIDTH};

pub mod emulator;
// pub mod rusticnes;
//use self::rusticnes::RusticNesState;
// pub type LocalNesState = RusticNesState;
pub mod tetanes;
use self::tetanes::TetanesNesState;
pub type LocalNesState = TetanesNesState;

pub struct NESBuffers {
    pub audio: NESAudioFrame,
    pub video: NESVideoFrame,
}
impl NESBuffers {
    pub fn new() -> Self {
        Self {
            audio: NESAudioFrame::new(),
            video: NESVideoFrame::new(),
        }
    }
}
pub struct NESVideoFrame(Vec<u8>);

impl NESVideoFrame {
    pub const SIZE: usize = (NES_WIDTH * NES_HEIGHT * 4) as usize;

    /// Allocate a new frame for video output.
    pub fn new() -> Self {
        let mut frame = vec![0; Self::SIZE];
        frame
            .iter_mut()
            .skip(3)
            .step_by(4)
            .for_each(|alpha| *alpha = 255);
        Self(frame)
    }
}

impl Default for NESVideoFrame {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for NESVideoFrame {
    type Target = Vec<u8>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NESVideoFrame {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct NESAudioFrame(Vec<f32>);
impl NESAudioFrame {
    fn new() -> NESAudioFrame {
        Self(Vec::new())
    }
}

impl Deref for NESAudioFrame {
    type Target = Vec<f32>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NESAudioFrame {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

static NTSC_PAL: &[u8] = include_bytes!("../../config/ntscpalette.pal");

pub trait NesStateHandler {
    fn advance(
        &mut self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        buffers: &mut Option<&mut NESBuffers>,
    );
    fn save_sram(&self) -> Option<Vec<u8>>;
    fn load_sram(&mut self, data: &mut Vec<u8>);
    fn frame(&self) -> u32;
}
