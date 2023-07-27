use crate::{
    input::JoypadInput,
    settings::{gui::GuiComponent, MAX_PLAYERS},
    Bundle, Fps, LocalStateHandler, StateHandler, FPS,
};
use serde::Deserialize;

use self::{
    connecting_state::{
        ConnectingState, NetplayServerConfiguration, ResumableNetplaySession, StartMethod,
    },
    gui::NetplayGui,
    netplay_state::{Disconnected, Netplay, NetplayState},
};

mod connecting_state;
mod gui;
mod netplay_session;
mod netplay_state;
#[cfg(feature = "debug")]
mod stats;

#[derive(Clone, Debug)]
pub struct InputMapping {
    pub ids: [usize; MAX_PLAYERS],
}
impl InputMapping {
    fn map(&self, local_input: usize) -> usize {
        self.ids[local_input]
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct NetplayBuildConfiguration {
    pub default_room_name: String,
    pub netplay_id: Option<String>,
    pub server: NetplayServerConfiguration,
}

pub struct NetplayStateHandler {
    pub netplay: Option<NetplayState>,
    local_state_handler: LocalStateHandler,
    pub gui: NetplayGui,
}

impl StateHandler for NetplayStateHandler {
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Fps {
        self.netplay = self.netplay.take().map(|netplay| netplay.advance(inputs));

        if let Some(netplay) = &self.netplay {
            match &netplay {
                NetplayState::Connected(netplay_connected) => {
                    netplay_connected.state.netplay_session.requested_fps
                }
                NetplayState::Disconnected(_) => self.local_state_handler.advance(inputs),
                _ => FPS,
            }
        } else {
            FPS
        }
    }

    fn consume_samples(&mut self) -> Vec<i16> {
        match &mut self.netplay.as_mut().unwrap() {
            NetplayState::Connected(netplay_connected) => netplay_connected
                .state
                .netplay_session
                .game_state
                .consume_samples(),
            NetplayState::Disconnected(_) => self.local_state_handler.consume_samples(),
            _ => vec![],
        }
    }

    fn get_frame(&self) -> Option<&Vec<u16>> {
        match &self.netplay.as_ref().unwrap() {
            NetplayState::Connected(netplay_connected) => Some(
                netplay_connected
                    .state
                    .netplay_session
                    .game_state
                    .get_frame(),
            ),
            NetplayState::Disconnected(_) => self.local_state_handler.get_frame(),
            _ => None,
        }
    }

    fn save(&self) -> Vec<u8> {
        if let NetplayState::Connected(netplay_connected) = &self.netplay.as_ref().unwrap() {
            //TODO: what to do when saving during netplay?
            netplay_connected.state.netplay_session.game_state.save()
        } else {
            self.local_state_handler.save()
        }
    }

    fn load(&mut self, data: &mut Vec<u8>) {
        if let NetplayState::Connected(netplay_connected) = &mut self.netplay.as_mut().unwrap() {
            //TODO: what to do when loading during netplay?
            netplay_connected
                .state
                .netplay_session
                .game_state
                .load(data);
        } else {
            self.local_state_handler.load(data);
        }
    }

    fn get_gui(&mut self) -> &mut dyn GuiComponent {
        //TODO: Would rather extend StateHandler with GuiComponent and do
        //      state_handler.as_mut() on the Box but couldn't due to
        //      https://github.com/rust-lang/rust/issues/65991
        self
    }
}

impl NetplayStateHandler {
    pub fn new(
        local_state_handler: LocalStateHandler,
        bundle: &Bundle,
        netplay_id: &mut Option<String>,
    ) -> Self {
        let netplay_build_config = &bundle.config.netplay;

        NetplayStateHandler {
            gui: NetplayGui::new(netplay_build_config.default_room_name.clone()),
            netplay: Some(NetplayState::Disconnected(Netplay::<Disconnected>::new(
                netplay_build_config.clone(),
                netplay_id,
                md5::compute(&bundle.rom),
                local_state_handler.state.clone(),
            ))),
            local_state_handler,
        }
    }
}
