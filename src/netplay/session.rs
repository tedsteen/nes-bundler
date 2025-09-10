use std::{
    fmt::Debug,
    mem,
    time::{Duration, Instant},
};

use anyhow::Error;
use ggrs::{Config, GgrsRequest, P2PSession, SessionBuilder};
use matchbox_socket::PeerId;
use tokio::select;

use crate::{
    emulation::{LocalNesState, NESBuffers, NesStateHandler, new_local_nes_state},
    input::JoypadState,
    netplay::{
        SharedNetplayConnectedState, SharedNetplayState,
        configuration::StaticNetplayServerConfiguration,
        connection::{ConnectingSession, NetplayConnection, StartMethod},
    },
    settings::{MAX_PLAYERS, Settings},
};
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

#[derive(Clone, Debug)]
pub struct NetplayNesState {
    pub nes_state: LocalNesState,
    pub ggrs_frame: i32,
    joypad_mapping: JoypadMapping,
}
impl NetplayNesState {
    pub fn new(joypad_mapping: JoypadMapping) -> Self {
        Self {
            nes_state: new_local_nes_state(false),
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

pub struct ConnectedNetplaySession {
    local_player_index: usize,
    pub p2p_session: P2PSession<GGRSConfig>,
    pub netplay_server_configuration: StaticNetplayServerConfiguration,

    pub last_handled_ggrs_frame: i32,
    pub current_game_state: NetplayNesState,
    pub last_confirmed_game_state1: NetplayNesState,
    pub last_confirmed_game_state2: NetplayNesState,
    start_time: Instant,
}

impl ConnectedNetplaySession {
    fn new(netplay_connection: NetplayConnection) -> Self {
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
            //TODO: Actual input mapping. This will only be needed when it's a random match (we don't have an obvious P1 & P2)
            NetplayNesState::new(if local_player_index == 0 {
                JoypadMapping::P1
            } else {
                JoypadMapping::P2
            })
        });

        Self {
            p2p_session,
            netplay_server_configuration,
            local_player_index,

            last_confirmed_game_state1: initial_state.clone(),
            last_confirmed_game_state2: initial_state.clone(),
            last_handled_ggrs_frame: -1,
            current_game_state: initial_state.clone(),
            start_time: Instant::now(),
        }
    }

    pub async fn advance(
        &mut self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        mut buffers: Option<NESBuffers<'_>>,
    ) {
        let p2p_session = &mut self.p2p_session;

        match p2p_session.current_state() {
            ggrs::SessionState::Synchronizing => {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            ggrs::SessionState::Running => {
                for handle in p2p_session.local_player_handles() {
                    p2p_session
                        .add_local_input(handle, *joypad_state[0])
                        .expect("Handle to be a local player");
                }

                #[cfg(feature = "debug")]
                puffin::profile_scope!("ggrs advance_frame");
                match p2p_session.advance_frame() {
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
                                            if is_replay { None } else { buffers.take() },
                                        )
                                        .await;

                                    if !is_replay {
                                        //This is not a replay
                                        self.last_handled_ggrs_frame =
                                            self.current_game_state.ggrs_frame;
                                        if self.current_game_state.ggrs_frame
                                            % (p2p_session.max_prediction() + 1) as i32
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
                            "Frame {} skipped: {e:?}",
                            self.current_game_state.ggrs_frame,
                        )
                    }
                }

                if p2p_session.frames_ahead() > 0 {
                    //https://www.desmos.com/calculator/zbntsowijd
                    let speed = 0.8_f32
                        .max(1.0 - 0.1 * (0.2 * p2p_session.frames_ahead() as f32).powf(2.0));
                    log::trace!(
                        "Frames ahead: {:?}, slowing down emulation ({speed}x)",
                        p2p_session.frames_ahead()
                    );

                    self.current_game_state.nes_state.set_speed(speed);
                } else {
                    self.current_game_state.nes_state.set_speed(1.0)
                }
            }
        }
    }

    fn to_shared_state(&self) -> SharedNetplayConnectedState {
        match self.p2p_session.current_state() {
            ggrs::SessionState::Synchronizing => SharedNetplayConnectedState::Synchronizing,
            ggrs::SessionState::Running => SharedNetplayConnectedState::Running(self.start_time),
        }
    }
}

#[derive(Debug)]
pub struct DisconnectedNetplaySession {
    local_nes_state: LocalNesState,
}
impl DisconnectedNetplaySession {
    fn new(local_play_nes_state: crate::emulation::tetanes::TetanesNesState) -> Self {
        Self {
            local_nes_state: local_play_nes_state,
        }
    }
}

pub struct ConnectingNetplaySession {
    pub connecting_session: ConnectingSession,
}

impl ConnectingNetplaySession {
    fn new(connecting_session: ConnectingSession) -> Self {
        Self { connecting_session }
    }
}
pub struct ResumingNetplaySession {
    attempt1: ConnectingSession,
    attempt2: ConnectingSession,
}
impl ResumingNetplaySession {
    fn new(session_state: &mut ConnectedNetplaySession) -> Self {
        //TODO: Popup/info about the error? Or perhaps put the reason for the resume in the resume state below?
        log::debug!(
            "Resuming netplay to one of the frames {:?} and {:?}",
            session_state.last_confirmed_game_state1.ggrs_frame,
            session_state.last_confirmed_game_state2.ggrs_frame
        );
        //let _ = connected.session_state.shared_state_sender.send(SharedNetplayState::Resuming);

        let netplay_server_configuration = &session_state.netplay_server_configuration;
        Self {
            attempt1: ConnectingSession::connect(StartMethod::Resume(
                netplay_server_configuration.clone(),
                session_state.last_confirmed_game_state1.clone(),
            )),
            attempt2: ConnectingSession::connect(StartMethod::Resume(
                netplay_server_configuration.clone(),
                session_state.last_confirmed_game_state2.clone(),
            )),
        }
    }
}
#[derive(Debug)]
pub struct FailedNetplaySession {
    reason: String,
    pub start_method: StartMethod,
}
impl FailedNetplaySession {
    fn new(reason: String, start_method: StartMethod) -> Self {
        Self {
            reason,
            start_method,
        }
    }
}
pub enum NetplaySession {
    Disconnected(DisconnectedNetplaySession),
    Connecting(ConnectingNetplaySession),
    Connected(ConnectedNetplaySession),
    Resuming(ResumingNetplaySession),
    Failed(FailedNetplaySession),
}

impl NetplaySession {
    pub(crate) fn new(local_play_nes_state: LocalNesState) -> Self {
        Self::Disconnected(DisconnectedNetplaySession::new(local_play_nes_state))
    }

    pub(crate) fn start(start_method: StartMethod) -> NetplaySession {
        let connecting_session = ConnectingSession::connect(start_method);

        Self::Connecting(ConnectingNetplaySession::new(connecting_session))
    }

    fn connect(connection: NetplayConnection) -> Self {
        Self::Connected(ConnectedNetplaySession::new(connection))
    }

    pub fn resume(session_state: &mut ConnectedNetplaySession) -> NetplaySession {
        Self::Resuming(ResumingNetplaySession::new(session_state))
    }
    fn fail(error: Error, start_method: StartMethod) -> Self {
        Self::Failed(FailedNetplaySession::new(error.to_string(), start_method))
    }

    pub(crate) fn to_shared_state(&self) -> SharedNetplayState {
        match self {
            NetplaySession::Disconnected(..) => SharedNetplayState::Disconnected,
            NetplaySession::Connecting(connecting_netplay_session) => {
                SharedNetplayState::Connecting(
                    connecting_netplay_session.connecting_session.state.clone(),
                )
            }
            NetplaySession::Connected(connected_netplay_session) => {
                SharedNetplayState::Connected(connected_netplay_session.to_shared_state())
            }
            NetplaySession::Resuming(..) => SharedNetplayState::Resuming,
            NetplaySession::Failed(failed_session) => {
                SharedNetplayState::Failed(failed_session.reason.to_string())
            }
        }
    }
}

const POLLING_TIMEOUT: Duration = Duration::from_millis(1);
impl NesStateHandler for NetplaySession {
    async fn advance(
        &mut self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        buffers: Option<NESBuffers<'_>>,
    ) {
        match self {
            NetplaySession::Disconnected(disconnected_session) => {
                disconnected_session
                    .local_nes_state
                    .advance(joypad_state, buffers)
                    .await;
            }
            NetplaySession::Connecting(connecting_netplay_session) => {
                let future_connection = &mut connecting_netplay_session
                    .connecting_session
                    .netplay_connection;

                select! {
                    _ = tokio::time::sleep(POLLING_TIMEOUT) => {},
                    connection_result = future_connection => {
                        match connection_result {
                            Ok(connection) => {
                                *self = NetplaySession::connect(connection);
                            },
                            Err(e) => {
                                *self = NetplaySession::fail(e, connecting_netplay_session.connecting_session.start_method.clone());
                            },
                        }

                    }
                };
            }
            NetplaySession::Connected(session_state) => {
                let p2p_session = &mut session_state.p2p_session;
                p2p_session.poll_remote_clients();

                if p2p_session
                    .events()
                    .any(|e| matches!(e, ggrs::GgrsEvent::Disconnected { .. }))
                {
                    log::warn!("Peer disconnected, resuming...");
                    *self = NetplaySession::resume(session_state);
                } else {
                    session_state.advance(joypad_state, buffers).await;
                }
            }
            NetplaySession::Resuming(resuming_netplay_session) => {
                //TODO: Handle if both fails
                tokio::select! {
                    _ = tokio::time::sleep(POLLING_TIMEOUT) => {},
                    Ok(c) = &mut resuming_netplay_session.attempt1.netplay_connection => {
                        *self = NetplaySession::connect(c);
                    }
                    Ok(c) = &mut resuming_netplay_session.attempt2.netplay_connection => {
                        *self = NetplaySession::connect(c);
                    }
                }
            }
            NetplaySession::Failed(..) => {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
    }

    fn reset(&mut self, hard: bool) {
        if let NetplaySession::Disconnected(disconnected_session) = self {
            disconnected_session.local_nes_state.reset(hard);
        }
    }

    fn set_speed(&mut self, speed: f32) {
        match self {
            NetplaySession::Connecting(..)
            | NetplaySession::Resuming(..)
            | NetplaySession::Failed(..) => {
                //Noop when connecting, resuming or failed
            }
            NetplaySession::Connected(s) => s.current_game_state.nes_state.set_speed(speed),
            NetplaySession::Disconnected(disconnected_session) => {
                disconnected_session.local_nes_state.set_speed(speed)
            }
        }
    }

    fn save_sram(&self) -> Option<&[u8]> {
        if let NetplaySession::Disconnected(disconnected_session) = self {
            disconnected_session.local_nes_state.save_sram()
        } else {
            None
        }
    }

    fn frame(&self) -> u32 {
        match self {
            NetplaySession::Connecting(..)
            | NetplaySession::Resuming(..)
            | NetplaySession::Failed(..) => 0,
            NetplaySession::Connected(s) => s.current_game_state.nes_state.frame(),
            NetplaySession::Disconnected(disconnected_session) => {
                disconnected_session.local_nes_state.frame()
            }
        }
    }
}

impl Debug for ConnectedNetplaySession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectedNetplaySession")
            .field("local_player_index", &self.local_player_index)
            .field(
                "p2p_session.current_state()",
                &self.p2p_session.current_state(),
            )
            .field("last_handled_ggrs_frame", &self.last_handled_ggrs_frame)
            .field("current_game_state", &self.current_game_state)
            .field(
                "last_confirmed_game_state1",
                &self.last_confirmed_game_state1,
            )
            .field(
                "last_confirmed_game_state2",
                &self.last_confirmed_game_state2,
            )
            .field("start_time", &self.start_time)
            .finish()
    }
}

impl Debug for ConnectingNetplaySession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectingNetplaySession")
            .field("connecting_session", &self.connecting_session)
            .finish()
    }
}

impl Debug for ResumingNetplaySession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResumingNetplaySession")
            .field("attempt1", &self.attempt1)
            .field("attempt2", &self.attempt2)
            .finish()
    }
}

impl Debug for NetplaySession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disconnected(arg0) => f.debug_tuple("Disconnected").field(&arg0).finish(),
            Self::Connecting(arg0) => f.debug_tuple("Connecting").field(&arg0).finish(),
            Self::Connected(arg0) => f.debug_tuple("Connected").field(arg0).finish(),
            Self::Resuming(arg0) => f.debug_tuple("Resuming").field(arg0).finish(),
            Self::Failed(arg0) => f.debug_tuple("Failed").field(arg0).finish(),
        }
    }
}
