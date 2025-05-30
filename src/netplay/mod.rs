use std::ops::{Deref, DerefMut};

use crate::{
    emulation::{LocalNesState, NESBuffers, NesStateHandler},
    input::JoypadState,
    settings::MAX_PLAYERS,
};
use anyhow::Result;

use self::{
    connecting_state::{ConnectingState, StartMethod, StartState},
    netplay_state::{Netplay, NetplayState},
};

pub mod configuration;
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
        #[cfg(feature = "debug")]
        if let Some(NetplayState::Connected(netplay)) = &mut self.netplay {
            let sess = &netplay.state.netplay_session.p2p_session;
            if netplay.state.netplay_session.game_state.frame % 30 == 0 {
                puffin::profile_scope!("Netplay stats");
                for i in 0..MAX_PLAYERS {
                    if let Ok(stats) = sess.network_stats(i) {
                        if !sess.local_player_handles().contains(&i) {
                            netplay.state.stats[i].push_stats(stats);
                        }
                    }
                }
            };
        }

        if let Some(new_state) = self
            .netplay
            .take()
            .map(|netplay| netplay.advance(joypad_state, buffers))
        {
            self.netplay = Some(new_state);
        }
    }

    fn save_sram(&self) -> Option<&[u8]> {
        // Saving is only supported when disconnected
        match &self.netplay {
            Some(NetplayState::Disconnected(s)) => s.state.save_sram(),
            _ => None,
        }
    }

    #[cfg(feature = "debug")]
    fn frame(&self) -> u32 {
        match &self.netplay {
            Some(NetplayState::Connected(s)) => s.state.netplay_session.game_state.frame(),
            Some(NetplayState::Disconnected(s)) => s.state.frame(),
            _ => 0,
        }
    }

    fn set_speed(&mut self, speed: f32) {
        match &mut self.netplay {
            Some(NetplayState::Connected(s)) => s.state.netplay_session.game_state.set_speed(speed),
            Some(NetplayState::Disconnected(s)) => s.state.set_speed(speed),
            _ => {}
        }
    }

    fn reset(&mut self, hard: bool) {
        match &mut self.netplay {
            Some(NetplayState::Connected(s)) => s.state.netplay_session.game_state.reset(hard),
            Some(NetplayState::Disconnected(s)) => s.state.reset(hard),
            _ => {}
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
