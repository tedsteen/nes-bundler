use egui::Context;

use crate::{
    input::JoypadInput,
    settings::{
        gui::{GuiComponent, GuiEvent},
        Settings, MAX_PLAYERS,
    },
    Fps, FPS,
};

use super::{NesState, NesStateHandler};

impl NesStateHandler for NesState {
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Fps {
        self.p1_input = inputs[0].0;
        self.p2_input = inputs[1].0;
        self.run_until_vblank();
        FPS
    }

    fn save(&self) -> Vec<u8> {
        self.save_state()
    }
    fn load(&mut self, data: &mut Vec<u8>) {
        self.load_state(data);
    }

    fn consume_samples(&mut self) -> Vec<i16> {
        self.apu.consume_samples()
    }

    fn get_frame(&self) -> Option<Vec<u16>> {
        Some(self.ppu.screen.clone())
    }
    fn get_gui(&mut self) -> &mut dyn GuiComponent {
        self
    }
}

impl GuiComponent for NesState {
    fn ui(&mut self, _ctx: &Context, _ui_visible: bool, _name: String, _settings: &mut Settings) {}
    fn event(&mut self, _event: &GuiEvent, _settings: &mut Settings) {}
    fn name(&self) -> Option<String> {
        None
    }
    fn open(&mut self) -> &mut bool {
        &mut self.1
    }
}
