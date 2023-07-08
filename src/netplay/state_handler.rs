use crate::{input::JoypadInput, settings::MAX_PLAYERS, Bundle, Fps, LocalGameState, StateHandler};

use super::{gui::NetplayGui, Netplay, NetplayState};

pub struct NetplayStateHandler {
    pub netplay: Netplay,
    local_game_state: LocalGameState,
    pub gui: NetplayGui,
}

impl StateHandler for NetplayStateHandler {
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Fps {
        if let Some(game_state) = self.netplay.advance(inputs) {
            // Continue the local game where the network game left of
            self.local_game_state = game_state;
        }

        if let NetplayState::Connected(netplay_session, _) = &self.netplay.state {
            netplay_session.requested_fps
        } else {
            self.local_game_state.advance(inputs)
        }
    }

    fn consume_samples(&mut self) -> Vec<i16> {
        if let NetplayState::Connected(netplay_session, _) = &mut self.netplay.state {
            netplay_session.game_state.consume_samples()
        } else {
            self.local_game_state.consume_samples()
        }
    }

    fn get_frame(&self) -> &Vec<u16> {
        if let NetplayState::Connected(netplay_session, _) = &self.netplay.state {
            netplay_session.game_state.get_frame()
        } else {
            self.local_game_state.get_frame()
        }
    }

    fn save(&self) -> Vec<u8> {
        if let NetplayState::Connected(netplay_session, _) = &self.netplay.state {
            //TODO: what to do when saving during netplay?
            netplay_session.game_state.save()
        } else {
            self.local_game_state.save()
        }
    }

    fn load(&mut self, data: &mut Vec<u8>) {
        if let NetplayState::Connected(netplay_session, _) = &mut self.netplay.state {
            //TODO: what to do when loading during netplay?
            netplay_session.game_state.load(data);
        } else {
            self.local_game_state.load(data);
        }
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

        NetplayStateHandler {
            local_game_state: initial_game_state.clone(),
            gui: NetplayGui::new(netplay_build_config.default_room_name.clone()),
            netplay: Netplay::new(
                netplay_build_config.clone(),
                netplay_id,
                md5::compute(&bundle.rom),
                initial_game_state,
            ),
        }
    }
}
