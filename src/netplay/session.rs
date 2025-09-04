use std::{
    mem,
    time::{Duration, Instant},
};

use anyhow::Result;
use ggrs::{Config, GgrsRequest, P2PSession, SessionBuilder};
use matchbox_socket::PeerId;

use crate::{
    emulation::{LocalNesState, NESBuffers, NesStateHandler, new_local_nes_state},
    input::JoypadState,
    netplay::{configuration::StaticNetplayServerConfiguration, connection::NetplayConnection},
    settings::{MAX_PLAYERS, Settings},
};
#[derive(Clone, Debug)]
enum JoypadMapping {
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

#[derive(Clone)]
pub struct NetplayNesState {
    pub nes_state: LocalNesState,
    pub ggrs_frame: i32,
    joypad_mapping: JoypadMapping,
}
impl NetplayNesState {
    fn new(nes_state: LocalNesState, joypad_mapping: JoypadMapping) -> Self {
        Self {
            nes_state,
            ggrs_frame: 0,
            joypad_mapping,
        }
    }
}

#[derive(Debug)]
pub struct GGRSConfig;
impl Config for GGRSConfig {
    type Input = u8;
    type State = NetplayNesState;
    type Address = PeerId;
}

pub struct NetplaySession {
    local_player_index: usize,
    pub p2p_session: P2PSession<GGRSConfig>,
    pub netplay_server_configuration: StaticNetplayServerConfiguration,

    pub last_handled_ggrs_frame: i32,
    pub current_game_state: NetplayNesState,
    pub last_confirmed_game_state1: NetplayNesState,
    pub last_confirmed_game_state2: NetplayNesState,
    pub last_running: Instant,
}

pub enum AdvanceError {
    LostPeer,
}

impl NetplaySession {
    pub fn new(netplay_connection: NetplayConnection) -> Self {
        let mut socket = netplay_connection.socket;
        let netplay_server_configuration = netplay_connection.netplay_server_configuration.clone();
        let initial_state = netplay_connection.initial_state;

        let ggrs_config = &netplay_server_configuration.ggrs;
        let mut sess_build = SessionBuilder::<GGRSConfig>::new()
            .with_num_players(MAX_PLAYERS)
            .with_input_delay(ggrs_config.input_delay)
            .with_fps(Settings::current_mut().get_nes_region().to_fps() as usize)
            .unwrap()
            .with_max_prediction_window(ggrs_config.max_prediction);

        let players = socket.players();
        for (i, player) in players.into_iter().enumerate() {
            sess_build = sess_build
                .add_player(player, i)
                .expect("player to be added to ggrs session");
        }

        let p2p_session = sess_build
            .start_p2p_session(socket.take_channel(0).expect("a channel"))
            .expect("ggrs session to start");
        //There should be only one.
        let local_player_index = *p2p_session.local_player_handles().first().unwrap_or(&0);

        let initial_state = initial_state.unwrap_or_else(|| {
            //TODO: Actual input mapping (or at least let the host be P1 in host/join)
            let joypad_mapping = if local_player_index == 0 {
                JoypadMapping::P1
            } else {
                JoypadMapping::P2
            };
            NetplayNesState::new(new_local_nes_state(), joypad_mapping)
        });

        Self {
            p2p_session,
            netplay_server_configuration,
            local_player_index,

            last_confirmed_game_state1: initial_state.clone(),
            last_confirmed_game_state2: initial_state.clone(),
            last_handled_ggrs_frame: -1,
            current_game_state: initial_state.clone(),
            last_running: Instant::now(),
        }
    }

    pub async fn advance(
        &mut self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        buffers: &mut NESBuffers<'_>,
    ) -> Result<(), AdvanceError> {
        let p2p_session = &mut self.p2p_session;
        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("ggrs poll_remote_clients");

            p2p_session.poll_remote_clients();
        }

        match &p2p_session.current_state() {
            ggrs::SessionState::Synchronizing => {
                tokio::time::sleep(Duration::from_millis(16)).await;
            }
            ggrs::SessionState::Running => {
                self.last_running = Instant::now();

                let sess = p2p_session;

                for event in sess.events() {
                    if let ggrs::GgrsEvent::Disconnected { .. } = event {
                        return Err(AdvanceError::LostPeer);
                    }
                }

                for handle in sess.local_player_handles() {
                    sess.add_local_input(handle, *joypad_state[0])
                        .expect("Handle to be a local player");
                }

                #[cfg(feature = "debug")]
                puffin::profile_scope!("ggrs advance_frame");
                match sess.advance_frame() {
                    Ok(requests) => {
                        for request in requests {
                            match request {
                                GgrsRequest::LoadGameState { cell, frame } => {
                                    log::debug!("Loading (frame {:?})", frame);
                                    self.current_game_state =
                                        cell.load().expect("ggrs state to load");
                                }
                                GgrsRequest::SaveGameState { cell, frame } => {
                                    assert_eq!(self.current_game_state.ggrs_frame, frame);
                                    cell.save(frame, Some(self.current_game_state.clone()), None);
                                }
                                GgrsRequest::AdvanceFrame { inputs } => {
                                    let is_replay = self.current_game_state.ggrs_frame
                                        <= self.last_handled_ggrs_frame;
                                    let no_buffers = &mut NESBuffers {
                                        audio: None,
                                        video: None,
                                    };

                                    self.current_game_state
                                        .nes_state
                                        .advance(
                                            self.current_game_state.joypad_mapping.map(
                                                [
                                                    JoypadState(inputs[0].0),
                                                    JoypadState(inputs[1].0),
                                                ],
                                                self.local_player_index,
                                            ),
                                            if is_replay { no_buffers } else { buffers },
                                        )
                                        .await;

                                    if !is_replay {
                                        //This is not a replay
                                        self.last_handled_ggrs_frame =
                                            self.current_game_state.ggrs_frame;
                                        if self.current_game_state.ggrs_frame
                                            % (sess.max_prediction() + 1) as i32
                                            == 0
                                        {
                                            mem::swap(
                                                &mut self.last_confirmed_game_state1,
                                                &mut self.last_confirmed_game_state2,
                                            );
                                            self.last_confirmed_game_state2 =
                                                self.current_game_state.clone()
                                        }
                                    }

                                    self.current_game_state.ggrs_frame += 1;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "Frame {} skipped: {:?}",
                            self.current_game_state.ggrs_frame,
                            e
                        )
                    }
                }

                if sess.frames_ahead() > 0 {
                    //https://www.desmos.com/calculator/zbntsowijd
                    let speed =
                        0.8_f32.max(1.0 - 0.1 * (0.2 * sess.frames_ahead() as f32).powf(2.0));
                    log::trace!(
                        "Frames ahead: {:?}, slowing down emulation ({speed}x)",
                        sess.frames_ahead()
                    );

                    self.current_game_state.nes_state.set_speed(speed);
                } else {
                    self.current_game_state.nes_state.set_speed(1.0)
                }
            }
        };
        Ok(())
    }
}
