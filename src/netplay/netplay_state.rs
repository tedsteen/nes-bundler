use std::rc::Rc;

use md5::Digest;
use tokio::runtime::{Builder, Runtime};
use uuid::Uuid;

use crate::{input::JoypadInput, settings::MAX_PLAYERS};

use super::{
    netplay_session::NetplaySession, ConnectingState, JoypadMapping, NetplayBuildConfiguration,
    NetplayNesState, StartMethod, StartState,
};

pub enum NetplayState {
    Disconnected(Netplay<()>),
    Connecting(Netplay<ConnectingState>),
    Connected(Netplay<Connected>),
    Resuming(Netplay<Resuming>),
    Failed(Netplay<Failed>),
}

pub struct Failed {
    pub reason: String,
}

impl NetplayState {
    pub fn advance(self, inputs: [JoypadInput; MAX_PLAYERS]) -> Self {
        use NetplayState::*;
        match self {
            Connecting(netplay) => netplay.advance(),
            Connected(netplay) => netplay.advance(inputs),
            Resuming(netplay) => netplay.advance(),
            Failed(_) | NetplayState::Disconnected(_) => self,
        }
    }
}

pub struct Netplay<S> {
    pub rt: Rc<Runtime>,
    pub config: NetplayBuildConfiguration,
    pub netplay_id: String,
    pub rom_hash: Digest,
    pub initial_game_state: NetplayNesState,
    pub state: S,
}

impl<T> Netplay<T> {
    fn from<S>(state: T, other: Netplay<S>) -> Self {
        Self {
            rt: other.rt,
            config: other.config,
            netplay_id: other.netplay_id,
            rom_hash: other.rom_hash,
            initial_game_state: other.initial_game_state,
            state,
        }
    }

    pub fn disconnect(self) -> Netplay<()> {
        log::debug!("Disconnecting");
        Netplay::new(
            self.config,
            &mut Some(self.netplay_id),
            self.rom_hash,
            self.initial_game_state,
        )
    }
}

pub struct Connected {
    pub netplay_session: NetplaySession,
    session_id: String,
}

pub struct Resuming {
    attempt1: ConnectingState,
    attempt2: ConnectingState,
}
impl Resuming {
    fn new(netplay: &mut Netplay<Connected>) -> Self {
        let netplay_session = &netplay.state.netplay_session;

        let session_id = netplay.state.session_id.clone();
        Self {
            attempt1: ConnectingState::connect(
                netplay,
                StartMethod::Resume(StartState {
                    game_state: netplay_session.last_confirmed_game_states[1].clone(),
                    session_id: session_id.clone(),
                }),
            ),
            attempt2: ConnectingState::connect(
                netplay,
                StartMethod::Resume(StartState {
                    game_state: netplay_session.last_confirmed_game_states[0].clone(),
                    session_id,
                }),
            ),
        }
    }
}

impl Netplay<()> {
    pub fn new(
        config: NetplayBuildConfiguration,
        netplay_id: &mut Option<String>,
        rom_hash: Digest,
        initial_game_state: NetplayNesState,
    ) -> Self {
        Self {
            rt: Rc::new(
                Builder::new_multi_thread()
                    .enable_all()
                    .thread_name("netplay-pool")
                    .build()
                    .expect("Could not create an async runtime for Netplay"),
            ),
            config,
            netplay_id: netplay_id
                .get_or_insert_with(|| Uuid::new_v4().to_string())
                .to_string(),
            rom_hash,
            initial_game_state,
            state: (),
        }
    }

    pub fn join_by_name(self, room_name: &str) -> NetplayState {
        let initial_state = self.initial_game_state.clone();
        let session_id = format!("{}_{:x}", room_name, self.rom_hash);
        self.join(StartMethod::Join(
            StartState {
                game_state: initial_state,
                session_id,
            },
            room_name.to_string(),
        ))
    }

    pub fn match_with_random(self) -> NetplayState {
        let initial_state = self.initial_game_state.clone();
        // TODO: When resuming using this session id there might be collisions, but it's unlikely.
        //       Should be fixed though.
        let session_id = format!("{:x}", self.rom_hash);
        self.join(StartMethod::MatchWithRandom(StartState {
            game_state: initial_state,
            session_id,
        }))
    }

    pub fn join(self, start_method: StartMethod) -> NetplayState {
        log::debug!("Joining: {:?}", start_method);
        NetplayState::Connecting(Netplay::from(
            ConnectingState::connect(&self, start_method),
            self,
        ))
    }
}

impl Netplay<ConnectingState> {
    pub fn cancel(self) -> Netplay<()> {
        log::debug!("Connection cancelled by user");
        self.disconnect()
    }

    fn advance(mut self) -> NetplayState {
        //log::trace!("Advancing Netplay<ConnectingState>");
        self.state = self.state.advance();
        match self.state {
            ConnectingState::Connected(connected) => {
                log::debug!("Connected! Starting netplay session");
                NetplayState::Connected(Netplay {
                    rt: self.rt,
                    config: self.config,
                    netplay_id: self.netplay_id,
                    rom_hash: self.rom_hash,
                    initial_game_state: self.initial_game_state,
                    state: Connected {
                        netplay_session: connected.state,
                        session_id: match connected.start_method {
                            StartMethod::Join(StartState { session_id, .. }, _)
                            | StartMethod::MatchWithRandom(StartState { session_id, .. })
                            | StartMethod::Resume(StartState { session_id, .. }) => session_id,
                        },
                    },
                })
            }
            ConnectingState::Failed(reason) => NetplayState::Failed(Netplay {
                rt: self.rt,
                config: self.config,
                netplay_id: self.netplay_id,
                rom_hash: self.rom_hash,
                initial_game_state: self.initial_game_state,
                state: Failed { reason },
            }),
            _ => NetplayState::Connecting(self),
        }
    }
}

impl Netplay<Connected> {
    pub fn resume(mut self) -> Netplay<Resuming> {
        log::debug!(
            "Resuming netplay to one of the frames ({:?})",
            self.state
                .netplay_session
                .last_confirmed_game_states
                .clone()
                .map(|s| s.frame)
        );

        Netplay::from(Resuming::new(&mut self), self)
    }

    fn advance(mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> NetplayState {
        //log::trace!("Advancing Netplay<Connected>");
        if let Some(joypad_mapping) =
            &mut self.state.netplay_session.game_state.joypad_mapping.clone()
        {
            if self
                .state
                .netplay_session
                .advance(inputs, joypad_mapping)
                .is_err()
            {
                //TODO: Popup/info about the error? Or perhaps put the reason for the resume in the resume state below?
                NetplayState::Resuming(self.resume())
            } else {
                NetplayState::Connected(self)
            }
        } else {
            //TODO: Actual input mapping..
            self.state.netplay_session.game_state.joypad_mapping =
                Some(if self.state.netplay_session.get_local_player_idx() == 0 {
                    JoypadMapping::P1
                } else {
                    JoypadMapping::P2
                });
            NetplayState::Connected(self)
        }
    }
}

impl Netplay<Resuming> {
    fn advance(mut self) -> NetplayState {
        //log::trace!("Advancing Netplay<Resuming>");
        self.state.attempt1 = self.state.attempt1.advance();
        self.state.attempt2 = self.state.attempt2.advance();

        if let ConnectingState::Connected(_) = &self.state.attempt1 {
            NetplayState::Connecting(Netplay {
                rt: self.rt,
                config: self.config,
                netplay_id: self.netplay_id,
                rom_hash: self.rom_hash,
                initial_game_state: self.initial_game_state,
                state: self.state.attempt1,
            })
        } else if let ConnectingState::Connected(_) = &self.state.attempt2 {
            return NetplayState::Connecting(Netplay {
                rt: self.rt,
                config: self.config,
                netplay_id: self.netplay_id,
                rom_hash: self.rom_hash,
                initial_game_state: self.initial_game_state,
                state: self.state.attempt2,
            });
        } else {
            NetplayState::Resuming(self)
        }
    }

    pub fn cancel(self) -> Netplay<()> {
        log::debug!("Resume cancelled by user");
        self.disconnect()
    }
}

impl Netplay<Failed> {
    pub fn restart(self) -> Netplay<()> {
        self.disconnect()
    }
}
