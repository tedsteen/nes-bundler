use std::ops::{Deref, DerefMut};

use crate::{
    input::JoypadInput,
    nes_state::{local::LocalNesState, NesStateHandler},
    settings::MAX_PLAYERS,
    Bundle, Fps, FPS,
};
use serde::Deserialize;

use self::{
    connecting_state::{ConnectingState, NetplayServerConfiguration, StartMethod, StartState},
    netplay_state::{Netplay, NetplayState},
};

mod connecting_state;
pub mod gui;
mod netplay_session;
mod netplay_state;
#[cfg(feature = "debug")]
mod stats;

#[derive(Clone, Debug)]
pub enum JoypadMapping {
    P1,
    P2,
}

impl JoypadMapping {
    fn map(
        &self,
        inputs: [JoypadInput; MAX_PLAYERS],
        local_player_idx: usize,
    ) -> [JoypadInput; MAX_PLAYERS] {
        match self {
            JoypadMapping::P1 => {
                if local_player_idx == 0 {
                    [inputs[0], inputs[1]]
                } else {
                    [inputs[1], inputs[0]]
                }
            }
            JoypadMapping::P2 => {
                if local_player_idx == 0 {
                    [inputs[1], inputs[0]]
                } else {
                    [inputs[0], inputs[1]]
                }
            }
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct NetplayBuildConfiguration {
    pub default_room_name: String,
    pub netplay_id: Option<String>,
    pub server: NetplayServerConfiguration,
}

pub struct NetplayStateHandler {
    netplay: Option<NetplayState>,
    nes_state: LocalNesState,

    //Gui
    gui_is_open: bool,
    room_name: String,
}

#[derive(Clone)]
pub struct NetplayNesState {
    nes_state: LocalNesState,
    frame: i32,
    joypad_mapping: Option<JoypadMapping>,
}

impl Deref for NetplayNesState {
    type Target = LocalNesState;
    fn deref(&self) -> &LocalNesState {
        &self.nes_state
    }
}

impl DerefMut for NetplayNesState {
    fn deref_mut(&mut self) -> &mut LocalNesState {
        &mut self.nes_state
    }
}

impl NesStateHandler for NetplayStateHandler {
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Fps {
        self.netplay = self.netplay.take().map(|netplay| netplay.advance(inputs));

        if let Some(netplay) = &self.netplay {
            match &netplay {
                NetplayState::Connected(netplay_connected) => {
                    netplay_connected.state.netplay_session.requested_fps
                }
                NetplayState::Disconnected(_) => self.nes_state.advance(inputs),
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
            NetplayState::Disconnected(_) => self.nes_state.consume_samples(),
            _ => vec![],
        }
    }

    fn get_frame(&self) -> Option<Vec<u16>> {
        match &self.netplay.as_ref().unwrap() {
            NetplayState::Connected(netplay_connected) => netplay_connected
                .state
                .netplay_session
                .game_state
                .get_frame()
                .clone(),
            NetplayState::Disconnected(_) => self.nes_state.get_frame(),
            _ => None,
        }
    }

    fn save(&self) -> Vec<u8> {
        if let NetplayState::Connected(netplay_connected) = &self.netplay.as_ref().unwrap() {
            //TODO: what to do when saving during netplay?
            netplay_connected.state.netplay_session.game_state.save()
        } else {
            self.nes_state.save()
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
            self.nes_state.load(data);
        }
    }
    fn get_gui(&mut self) -> Option<&mut dyn crate::settings::gui::GuiComponent> {
        Some(self)
    }
}

impl NetplayStateHandler {
    pub fn new(nes_state: LocalNesState, bundle: &Bundle, netplay_id: &mut Option<String>) -> Self {
        let netplay_build_config = &bundle.config.netplay;

        NetplayStateHandler {
            netplay: Some(NetplayState::Disconnected(Netplay::new(
                netplay_build_config.clone(),
                netplay_id,
                md5::compute(&bundle.rom),
                NetplayNesState {
                    nes_state: nes_state.clone(),
                    frame: 0,
                    joypad_mapping: None,
                },
            ))),
            nes_state,
            gui_is_open: true,
            room_name: netplay_build_config.default_room_name.clone(),
        }
    }
}
