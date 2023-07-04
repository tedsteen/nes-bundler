use crate::{
    input::JoypadInput, settings::MAX_PLAYERS, Bundle, Fps, LocalGameState, StateHandler, FPS,
};

use super::{gui::NetplayGui, Netplay, NetplayState};

pub struct NetplayStateHandler {
    pub netplay: Netplay,
    game_state: LocalGameState,
    initial_game_state: LocalGameState,
    pub gui: NetplayGui,
}

impl StateHandler for NetplayStateHandler {
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Fps {
        if let Some(new_state) = self.netplay.advance(inputs) {
            if let NetplayState::Connected(..) = &new_state {
                self.game_state = self.initial_game_state.clone();
            }
            self.netplay.state = new_state;
        } else {
            match &mut self.netplay.state {
                NetplayState::Connected(netplay_session, _) => {
                    netplay_session.advance(&mut self.game_state, inputs);
                    return netplay_session.requested_fps;
                }
                _ => {
                    self.game_state.advance(inputs);
                }
            }
        }
        FPS
    }

    fn consume_samples(&mut self) -> Vec<i16> {
        self.game_state.consume_samples()
    }
    fn get_frame(&self) -> &Vec<u16> {
        self.game_state.get_frame()
    }
    fn save(&self) -> Vec<u8> {
        self.game_state.save()
    }
    fn load(&mut self, data: &mut Vec<u8>) {
        self.game_state.load(data)
    }

    fn get_gui(&mut self) -> &mut dyn crate::settings::gui::GuiComponent {
        //TODO: Would rather extend StateHandler with GuiComponent and do
        //      state_handler.as_mut() on the Box but couldn't due to
        //      https://github.com/rust-lang/rust/issues/65991
        self
    }
}

impl NetplayStateHandler {
    pub fn new(
        initial_game_state: LocalGameState,
        bundle: &Bundle,
        netplay_id: &mut Option<String>,
    ) -> Self {
        let netplay_build_config = &bundle.config.netplay;
        let netplay = Netplay::new(
            netplay_build_config.clone(),
            netplay_id,
            md5::compute(&bundle.rom),
        );

        NetplayStateHandler {
            netplay,
            game_state: initial_game_state.clone(),
            initial_game_state,
            gui: NetplayGui::new(netplay_build_config.default_room_name.clone()),
        }
    }
}
