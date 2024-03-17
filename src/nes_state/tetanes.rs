use rusticnes_core::palettes::NTSC_PAL;
use tetanes_core::{
    self,
    common::Reset,
    input::{Joypad, Player},
};

use super::{FrameData, LocalNesState, NesStateHandler};
use crate::{
    input::JoypadInput,
    settings::{gui::GuiComponent, MAX_PLAYERS},
};
type InternalNesState = tetanes_core::control_deck::ControlDeck;

#[derive(Clone)]
pub struct TetanesNesState {
    deck: tetanes_core::control_deck::ControlDeck,
}

impl TetanesNesState {
    pub fn load_rom(rom: &[u8]) -> LocalNesState {
        let mut deck = InternalNesState::new();

        deck.load_rom("Name", &mut rom.clone(), None)
            .expect("Could not load ROM");
        deck.reset(tetanes_core::common::ResetKind::Hard);

        TetanesNesState { deck }
    }
}

impl NesStateHandler for TetanesNesState {
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Option<FrameData> {
        *self.deck.joypad_mut(Player::One) = Joypad::signature((*inputs[0]).into());
        *self.deck.joypad_mut(Player::Two) = Joypad::signature((*inputs[1]).into());

        self.deck.clear_audio_samples();

        self.deck.clock_frame().expect("Failed to clock the NES");

        let audio = self.deck.audio_samples().to_vec();

        let video = self
            .deck
            .cpu()
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
            audio,
            fps: crate::FPS,
        })
    }

    fn save(&self) -> Option<Vec<u8>> {
        Some(bincode::serialize(&self.deck.cpu()).expect("Could not save state"))
    }
    fn load(&mut self, data: &mut Vec<u8>) {
        *self.deck.cpu_mut() = bincode::deserialize(data).expect("Could not load state");
    }

    fn get_gui(&mut self) -> Option<&mut dyn GuiComponent> {
        None
    }

    fn discard_samples(&mut self) {
        self.deck.clear_audio_samples();
    }
}
