use crate::{input::JoypadInput, settings::MAX_PLAYERS, Bundle, Fps, LocalGameState, StateHandler};

use super::{gui::NetplayGui, Netplay, NetplayState};

pub struct NetplayStateHandler {
    pub netplay: Netplay,
    local_game_state: LocalGameState,
    pub gui: NetplayGui,
}

impl StateHandler for NetplayStateHandler {
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Fps {
        self.netplay.advance(inputs);
        match &self.netplay.state {
            NetplayState::Connected(netplay_session) => netplay_session.requested_fps,
            NetplayState::Disconnected => self.local_game_state.advance(inputs),
            _ => crate::FPS,
        }
    }

    fn consume_samples(&mut self) -> Vec<i16> {
        match &mut self.netplay.state {
            NetplayState::Connected(netplay_session) => {
                netplay_session.game_state.consume_samples()
            }
            NetplayState::Disconnected => self.local_game_state.consume_samples(),
            _ => vec![],
        }
    }

    fn get_frame(&self) -> Option<&Vec<u16>> {
        match &self.netplay.state {
            NetplayState::Connected(netplay_session) => {
                Some(netplay_session.game_state.get_frame())
            }
            NetplayState::Disconnected => Some(self.local_game_state.get_frame()),
            _ => None,
        }
    }

    fn save(&self) -> Vec<u8> {
        if let NetplayState::Connected(netplay_session) = &self.netplay.state {
            //TODO: what to do when saving during netplay?
            netplay_session.game_state.save()
        } else {
            self.local_game_state.save()
        }
    }

    fn load(&mut self, data: &mut Vec<u8>) {
        if let NetplayState::Connected(netplay_session) = &mut self.netplay.state {
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
