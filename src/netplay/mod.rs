use std::ops::{Deref, DerefMut};

use crate::{
    input::JoypadInput,
    nes_state::{local::LocalNesState, start_nes, FrameData, NesStateHandler},
    settings::MAX_PLAYERS,
    Bundle,
};
use anyhow::Context;
use rusticnes_core::cartridge::mapper_from_file;
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
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Option<FrameData> {
        if let Some((new_state, frame_data)) =
            self.netplay.take().map(|netplay| netplay.advance(inputs))
        {
            self.netplay = Some(new_state);
            frame_data
        } else {
            None
        }
    }

    fn save(&self) -> Option<Vec<u8>> {
        //Saving is not supported in netplay
        None
    }

    fn load(&mut self, _data: &mut Vec<u8>) {
        //Loading is not supported in netplay
    }

    fn get_gui(&mut self) -> Option<&mut dyn crate::settings::gui::GuiComponent> {
        Some(self)
    }
}

impl NetplayStateHandler {
    pub fn new(
        start_local_nes: Box<dyn Fn() -> LocalNesState>,
        bundle: &Bundle,
        netplay_id: &mut Option<String>,
    ) -> Self {
        let netplay_build_config = &bundle.config.netplay;
        let netplay_rom = bundle.netplay_rom.clone();

        NetplayStateHandler {
            netplay: Some(NetplayState::Disconnected(Netplay::new(
                netplay_build_config.clone(),
                netplay_id,
                md5::compute(&netplay_rom),
                start_local_nes,
                Box::new(move || -> LocalNesState {
                    start_nes(
                        mapper_from_file(&netplay_rom)
                            .map_err(anyhow::Error::msg)
                            .context("Failed to load ROM")
                            .unwrap(),
                    )
                }),
            ))),
            gui_is_open: true,
            room_name: netplay_build_config.default_room_name.clone(),
        }
    }
}
