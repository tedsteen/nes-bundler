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

        let a = deck.load_rom("Name", &mut rom.clone(), None);
        deck.reset(tetanes_core::common::ResetKind::Hard);

        TetanesNesState { deck }
    }
}

impl NesStateHandler for TetanesNesState {
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Option<FrameData> {
        println!("TODO: Advance");
        // self.p1_input = *inputs[0];
        // self.p2_input = *inputs[1];
        // self.run_until_vblank();
        // Some(FrameData {
        //     video: self.ppu.screen.clone(),
        //     audio: self.apu.consume_samples(),
        //     fps: FPS,
        // })
        *self.deck.joypad_mut(Player::One) = Joypad::signature((*inputs[0]).into());
        *self.deck.joypad_mut(Player::Two) = Joypad::signature((*inputs[1]).into());

        self.deck.clock_frame().expect("Failed to clock the NES");

        let audio = self.deck.audio_samples().to_vec();

        let video = self.deck.frame_buffer().to_vec();
        Some(FrameData {
            video,
            audio,
            fps: crate::FPS,
        })
    }

    fn save(&self) -> Option<Vec<u8>> {
        println!("TODO: Save");
        //Some(self.save_state())
        None
    }
    fn load(&mut self, data: &mut Vec<u8>) {
        println!("TODO: Load");

        //self.load_state(data);
    }

    fn get_gui(&mut self) -> Option<&mut dyn GuiComponent> {
        None
    }

    fn discard_samples(&mut self) {
        todo!()
    }
}
