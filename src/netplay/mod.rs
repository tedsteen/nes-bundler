use std::ops::{Deref, DerefMut};

use crate::{
    input::JoypadState,
    nes_state::{LocalNesState, NESBuffers, NesStateHandler},
    settings::MAX_PLAYERS,
};
use anyhow::Result;
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
        joypad_state: [JoypadState; MAX_PLAYERS],
        local_player_idx: usize,
    ) -> [JoypadState; MAX_PLAYERS] {
        match self {
            JoypadMapping::P1 => {
                if local_player_idx == 0 {
                    [joypad_state[0], joypad_state[1]]
                } else {
                    [joypad_state[1], joypad_state[0]]
                }
            }
            JoypadMapping::P2 => {
                if local_player_idx == 0 {
                    [joypad_state[1], joypad_state[0]]
                } else {
                    [joypad_state[0], joypad_state[1]]
                }
            }
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct NetplayBuildConfiguration {
    pub netplay_id: Option<String>,
    pub server: NetplayServerConfiguration,
}

pub struct NetplayStateHandler {
    netplay: Option<NetplayState>,
}

#[derive(Clone)]
pub struct NetplayNesState {
    nes_state: LocalNesState,
    frame: i32,
    joypad_mapping: Option<JoypadMapping>,
}

impl NetplayNesState {
    fn new(nes_state: LocalNesState) -> Self {
        Self {
            nes_state,
            frame: 0,
            joypad_mapping: None,
        }
    }
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
    fn advance(&mut self, joypad_state: [JoypadState; MAX_PLAYERS], buffers: &mut NESBuffers) {
        if let Some(new_state) = self
            .netplay
            .take()
            .map(|netplay| netplay.advance(joypad_state, buffers))
        {
            self.netplay = Some(new_state);
        }
    }

    fn save_sram(&self) -> Option<Vec<u8>> {
        // Saving is only supported when disconnected
        match &self.netplay {
            Some(NetplayState::Disconnected(s)) => s.state.save_sram(),
            _ => None,
        }
    }

    fn load_sram(&mut self, data: &mut Vec<u8>) {
        // Loading is only supported when disconnected
        if let Some(NetplayState::Disconnected(s)) = &mut self.netplay {
            s.state.load_sram(data);
        }
    }

    fn frame(&self) -> u32 {
        match &self.netplay {
            Some(NetplayState::Connected(s)) => s.state.netplay_session.game_state.frame(),
            Some(NetplayState::Disconnected(s)) => s.state.frame(),
            _ => 0,
        }
    }
}

impl NetplayStateHandler {
    pub fn new() -> Result<Self> {
        Ok(NetplayStateHandler {
            netplay: Some(NetplayState::Disconnected(Netplay::new()?)),
        })
    }
}
