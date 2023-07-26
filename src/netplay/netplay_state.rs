use md5::Digest;
use tokio::runtime::{Builder, Runtime};
use uuid::Uuid;

use crate::{input::JoypadInput, settings::MAX_PLAYERS, LocalGameState};

use super::{
    connecting_state::Connecting, netplay_session::NetplaySession, ConnectingState, InputMapping,
    NetplayBuildConfiguration, ResumableNetplaySession, StartMethod,
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
    pub rt: Runtime,
    pub config: NetplayBuildConfiguration,
    pub netplay_id: String,
    pub rom_hash: Digest,
    pub initial_game_state: LocalGameState,
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
impl Resuming {
    fn new(netplay: &mut Netplay<Connected>) -> Self {
        let netplay_session = &netplay.state.netplay_session;
        let input_mapping = netplay_session.input_mapping.clone();

        let game_state_0 = netplay_session.last_confirmed_game_states[0].clone();
        let game_state_1 = netplay_session.last_confirmed_game_states[1].clone();

        Self {
            attempt1: Connecting::create(
                netplay,
                StartMethod::Resume(ResumableNetplaySession::new(
                    input_mapping.clone(),
                    game_state_0,
                )),
            ),
            attempt2: Connecting::create(
                netplay,
                StartMethod::Resume(ResumableNetplaySession::new(input_mapping, game_state_1)),
            ),
        }
    }
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
        Netplay::from(Connecting::create(&mut self, start_method), self)
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
        Netplay::from(Disconnected {}, self)
    }

    fn advance(mut self) -> NetplayState {
        self.state = self.state.advance(&mut self.rt, &self.rom_hash);
        match self.state {
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
            ConnectingState::Failed(_) => {
                NetplayState::Disconnected(Netplay::from(Disconnected {}, self))
            }
            _ => NetplayState::Connecting(self),
        }
    }
}

impl Netplay<Connected> {
    pub fn resume(mut self) -> Netplay<Resuming> {
        #[cfg(feature = "debug")]
        println!(
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
        if let Some(input_mapping) = self.state.netplay_session.input_mapping.clone() {
            if self
                .state
                .netplay_session
                .advance(inputs, &input_mapping)
                .is_err()
            {
                //TODO: Popup/info about the error? Or perhaps put the reason for the resume in the resume state below?
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
        Netplay::from(Disconnected {}, self)
    }
}

impl Netplay<Resuming> {
    fn advance(mut self) -> NetplayState {
        self.state.attempt1 = self.state.attempt1.advance(&mut self.rt, &self.rom_hash);
        self.state.attempt2 = self.state.attempt2.advance(&mut self.rt, &self.rom_hash);

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

    pub fn cancel(self) -> Netplay<Disconnected> {
        Netplay::from(Disconnected {}, self)
    }
}
