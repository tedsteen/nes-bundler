use md5::Digest;
use tokio::runtime::{Builder, Runtime};
use uuid::Uuid;

use crate::{input::JoypadInput, settings::MAX_PLAYERS, LocalGameState};

use super::{
    connecting::Connecting, ConnectingState, InputMapping, NetplayBuildConfiguration,
    NetplaySession, ResumableNetplaySession, StartMethod,
};

pub enum NetplayState {
    Disconnected(Netplay<Disconnected>),
    Connecting(Netplay<ConnectingState>),
    Connected(Netplay<Connected>),
    Resuming(Netplay<Resuming>),
}
impl NetplayState {
    pub fn advance(self, inputs: [JoypadInput; MAX_PLAYERS]) -> Self {
        match self {
            NetplayState::Disconnected(_) => self,
            NetplayState::Connecting(netplay) => netplay.advance(),
            NetplayState::Connected(netplay) => netplay.advance(inputs),
            NetplayState::Resuming(netplay) => netplay.advance(),
        }
    }
}

pub struct Netplay<S> {
    rt: Runtime,
    config: NetplayBuildConfiguration,
    netplay_id: String,
    rom_hash: Digest,
    initial_game_state: LocalGameState,
    pub state: S,
}

pub struct Disconnected {}

pub struct Connected {
    pub netplay_session: NetplaySession,
}

pub struct Resuming {
    attempt1: ConnectingState,
    attempt2: ConnectingState,
}

impl Netplay<Disconnected> {
    pub fn new(
        config: NetplayBuildConfiguration,
        netplay_id: &mut Option<String>,
        rom_hash: Digest,
        initial_game_state: LocalGameState,
    ) -> Self {
        Self {
            rt: Builder::new_multi_thread()
                .enable_all()
                .thread_name("netplay-pool")
                .build()
                .expect("Could not create an async runtime for Netplay"),
            config,
            netplay_id: netplay_id
                .get_or_insert_with(|| Uuid::new_v4().to_string())
                .to_string(),
            rom_hash,
            initial_game_state,
            state: Disconnected {},
        }
    }

    pub fn start(mut self, start_method: StartMethod) -> Netplay<ConnectingState> {
        Netplay::from(
            Connecting::create(
                &self.config.clone().server,
                &mut self.rt,
                &self.rom_hash,
                &self.netplay_id,
                start_method,
                self.initial_game_state.clone(),
            ),
            self,
        )
    }
}

impl<T> Netplay<T> {
    fn from<S>(state: T, other: Netplay<S>) -> Netplay<T> {
        Self {
            rt: other.rt,
            config: other.config,
            netplay_id: other.netplay_id,
            rom_hash: other.rom_hash,
            initial_game_state: other.initial_game_state,
            state,
        }
    }
}

impl Netplay<ConnectingState> {
    pub fn cancel(self) -> Netplay<Disconnected> {
        Netplay::new(
            self.config,
            &mut None,
            self.rom_hash,
            self.initial_game_state,
        )
    }

    fn advance(mut self) -> NetplayState {
        match self.state.advance(&mut self.rt, &self.rom_hash) {
            ConnectingState::Connected(connected) => NetplayState::Connected(Netplay {
                rt: self.rt,
                config: self.config,
                netplay_id: self.netplay_id,
                rom_hash: self.rom_hash,
                initial_game_state: self.initial_game_state,
                state: Connected {
                    netplay_session: connected.state,
                },
            }),
            ConnectingState::Failed(_) => NetplayState::Disconnected(Netplay {
                rt: self.rt,
                config: self.config,
                netplay_id: self.netplay_id,
                rom_hash: self.rom_hash,
                initial_game_state: self.initial_game_state,
                state: Disconnected {},
            }),
            state => NetplayState::Connecting(Netplay {
                rt: self.rt,
                config: self.config,
                netplay_id: self.netplay_id,
                rom_hash: self.rom_hash,
                initial_game_state: self.initial_game_state,
                state,
            }),
        }
    }
}

impl Netplay<Connected> {
    pub fn resume(mut self: Netplay<Connected>) -> Netplay<Resuming> {
        Netplay::from(
            Resuming {
                attempt1: Connecting::create(
                    &self.config.server,
                    &mut self.rt,
                    &self.rom_hash,
                    &self.netplay_id,
                    StartMethod::Resume(ResumableNetplaySession::new(
                        self.state.netplay_session.input_mapping.clone(),
                        self.state.netplay_session.last_confirmed_game_states[1].clone(),
                    )),
                    self.initial_game_state.clone(),
                ),
                attempt2: Connecting::create(
                    &self.config.server,
                    &mut self.rt,
                    &self.rom_hash,
                    &self.netplay_id,
                    StartMethod::Resume(ResumableNetplaySession::new(
                        self.state.netplay_session.input_mapping.clone(),
                        self.state.netplay_session.last_confirmed_game_states[0].clone(),
                    )),
                    self.initial_game_state.clone(),
                ),
            },
            self,
        )
    }

    fn advance(mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> NetplayState {
        if let Some(input_mapping) = self.state.netplay_session.input_mapping.clone() {
            if self
                .state
                .netplay_session
                .advance(inputs, &input_mapping)
                .is_err()
            {
                #[cfg(feature = "debug")]
                println!(
                    "Could not advance the Netplay session. Resuming to one of the frames ({:?})",
                    self.state
                        .netplay_session
                        .last_confirmed_game_states
                        .clone()
                        .map(|s| s.frame)
                );
                NetplayState::Resuming(self.resume())
            } else {
                NetplayState::Connected(self)
            }
        } else {
            //TODO: Actual input mapping..
            self.state.netplay_session.input_mapping = Some(InputMapping { ids: [0, 1] });
            NetplayState::Connected(self)
        }
    }
    pub(crate) fn disconnect(self) -> Netplay<Disconnected> {
        Netplay {
            rt: self.rt,
            config: self.config,
            netplay_id: self.netplay_id,
            rom_hash: self.rom_hash,
            initial_game_state: self.initial_game_state,
            state: Disconnected {},
        }
    }
}

impl Netplay<Resuming> {
    fn advance(mut self) -> NetplayState {
        self.state.attempt1 = self.state.attempt1.advance(&mut self.rt, &self.rom_hash);
        self.state.attempt2 = self.state.attempt2.advance(&mut self.rt, &self.rom_hash);

        if let ConnectingState::Connected(_) = &self.state.attempt1 {
            //TODO: Use From here?
            NetplayState::Connecting(Netplay {
                rt: self.rt,
                config: self.config,
                netplay_id: self.netplay_id,
                rom_hash: self.rom_hash,
                initial_game_state: self.initial_game_state,
                state: self.state.attempt1,
            })
        } else if let ConnectingState::Connected(_) = &self.state.attempt2 {
            //TODO: Use From here?
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

    pub fn cancel(self) -> Netplay<Disconnected> {
        Netplay::new(
            self.config,
            &mut None,
            self.rom_hash,
            self.initial_game_state,
        )
    }
}