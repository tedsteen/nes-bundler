use std::time::Instant;

use anyhow::Result;
use uuid::Uuid;

use crate::{
    bundle::Bundle,
    emulation::{LocalNesState, NESBuffers, NesStateHandler},
    input::JoypadState,
    settings::{Settings, MAX_PLAYERS},
};

use super::{
    connecting_state::JoinOrHost, netplay_session::NetplaySessionState, ConnectingState,
    JoypadMapping, StartMethod, StartState,
};

pub enum NetplayState {
    Disconnected(Netplay<LocalNesState>),
    Connecting(Netplay<ConnectingState>),
    Connected(Netplay<ConnectedState>),
    Resuming(Netplay<ResumingState>),
    Failed(Netplay<FailedState>),
}

pub struct FailedState {
    pub reason: String,
}

impl NetplayState {
    pub fn advance(
        self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        buffers: &mut NESBuffers,
    ) -> Self {
        use NetplayState::*;
        match self {
            Connecting(netplay) => {
                //No sound while connecting
                if let Some(audio) = &mut buffers.audio {
                    for _ in 0..1000 {
                        audio.push(0.0);
                    }
                }

                netplay.advance()
            }
            Connected(netplay) => netplay.advance(joypad_state, buffers),
            Resuming(netplay) => {
                //No sound while resuming
                if let Some(audio) = &mut buffers.audio {
                    for _ in 0..1000 {
                        audio.push(0.0);
                    }
                }

                netplay.advance()
            }
            Disconnected(netplay) => netplay.advance(joypad_state, buffers),
            Failed(netplay) => netplay.advance(),
        }
    }
}

pub struct Netplay<T> {
    pub state: T,
}
unsafe impl<T> Send for Netplay<T> {}

impl<T> Netplay<T> {
    fn from(state: T) -> Self {
        Self { state }
    }

    pub fn disconnect(self) -> Netplay<LocalNesState> {
        log::debug!("Disconnecting");
        Netplay::new().expect("disconnect to work")
    }
}

pub struct ConnectedState {
    pub netplay_session: NetplaySessionState,
    session_id: String,
    pub start_time: Instant,
    #[cfg(feature = "debug")]
    pub stats: [crate::netplay::stats::NetplayStats; crate::settings::MAX_PLAYERS],
}

pub struct ResumingState {
    attempt1: ConnectingState,
    attempt2: ConnectingState,
}
impl ResumingState {
    fn new(netplay: &mut Netplay<ConnectedState>) -> Self {
        let netplay_session = &netplay.state.netplay_session;

        let session_id = netplay.state.session_id.clone();
        Self {
            attempt2: ConnectingState::resume(
                netplay_session.last_confirmed_game_state2.clone(),
                session_id.clone(),
                netplay_session.netplay_server_configuration.clone(),
            ),
            attempt1: ConnectingState::resume(
                netplay_session.last_confirmed_game_state1.clone(),
                session_id.clone(),
                netplay_session.netplay_server_configuration.clone(),
            ),
        }
    }
}
pub fn get_netplay_id() -> String {
    Settings::current_mut()
        .netplay_id
        .get_or_insert_with(|| Uuid::new_v4().to_string())
        .to_string()
}

pub const MAX_ROOM_NAME_LEN: u8 = 4;

impl Netplay<LocalNesState> {
    pub fn new() -> Result<Self> {
        Ok(Self {
            state: LocalNesState::start_rom(
                &Bundle::current().rom,
                true,
                Settings::current_mut().get_nes_region(),
            )?,
        })
    }

    pub fn host_game(self) -> Result<NetplayState> {
        use rand::distr::{Alphanumeric, SampleString};

        let room_name = Alphanumeric
            .sample_string(&mut rand::rng(), MAX_ROOM_NAME_LEN.into())
            .to_uppercase();

        self.join_or_host(&room_name, JoinOrHost::Host)
    }

    pub fn join_game(self, room_name: &str) -> Result<NetplayState> {
        self.join_or_host(&room_name.to_uppercase(), JoinOrHost::Join)
    }

    fn join_or_host(self, room_name: &str, join_or_host: JoinOrHost) -> Result<NetplayState> {
        let netplay_rom = &Bundle::current().netplay_rom;
        let session_id = format!("{}_{:x}", room_name, md5::compute(netplay_rom));
        let nes_state = LocalNesState::start_rom(
            netplay_rom,
            false,
            Bundle::current().config.get_default_region(),
        )?;
        Ok(self.start(StartMethod::Start(
            StartState {
                game_state: super::NetplayNesState::new(nes_state),
                session_id,
            },
            room_name.to_string(),
            join_or_host,
        )))
    }

    pub fn find_game(self) -> Result<NetplayState> {
        let netplay_rom = &Bundle::current().netplay_rom;
        let rom_hash = md5::compute(netplay_rom);

        // TODO: When resuming using this session id there might be collisions, but it's unlikely.
        //       Should be fixed though.
        let session_id = format!("{:x}", rom_hash);
        let nes_state = LocalNesState::start_rom(
            netplay_rom,
            false,
            Bundle::current().config.get_default_region(),
        )?;
        Ok(self.start(StartMethod::MatchWithRandom(StartState {
            game_state: super::NetplayNesState::new(nes_state),
            session_id,
        })))
    }

    pub fn start(self, start_method: StartMethod) -> NetplayState {
        log::debug!("Starting: {:?}", start_method);
        NetplayState::Connecting(Netplay::from(ConnectingState::connect(start_method)))
    }

    fn advance(mut self, joypad_state: [JoypadState; 2], buffers: &mut NESBuffers) -> NetplayState {
        self.state.advance(joypad_state, buffers);
        NetplayState::Disconnected(self)
    }
}

impl Netplay<ConnectingState> {
    pub fn cancel(self) -> Netplay<LocalNesState> {
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
                    state: ConnectedState {
                        start_time: Instant::now(),
                        session_id: match &connected.start_method {
                            StartMethod::Start(StartState { session_id, .. }, ..)
                            | StartMethod::MatchWithRandom(StartState { session_id, .. })
                            | StartMethod::Resume(StartState { session_id, .. }) => {
                                session_id.clone()
                            }
                        },
                        netplay_session: connected,
                        #[cfg(feature = "debug")]
                        stats: [
                            crate::netplay::stats::NetplayStats::new(),
                            crate::netplay::stats::NetplayStats::new(),
                        ],
                    },
                })
            }
            ConnectingState::Failed(reason) => NetplayState::Failed(Netplay {
                state: FailedState { reason },
            }),
            _ => NetplayState::Connecting(self),
        }
    }
}

impl Netplay<ConnectedState> {
    pub fn resume(mut self) -> Netplay<ResumingState> {
        log::debug!(
            "Resuming netplay to one of the frames {:?} and {:?}",
            self.state.netplay_session.last_confirmed_game_state1.frame,
            self.state.netplay_session.last_confirmed_game_state2.frame
        );

        Netplay::from(ResumingState::new(&mut self))
    }

    fn advance(
        mut self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        buffers: &mut NESBuffers,
    ) -> NetplayState {
        //log::trace!("Advancing Netplay<Connected>");
        let netplay_session = &mut self.state.netplay_session;

        if let Some(joypad_mapping) = &mut netplay_session.game_state.joypad_mapping.clone() {
            match netplay_session.advance(joypad_state, joypad_mapping, buffers) {
                Ok(_) => NetplayState::Connected(self),
                Err(e) => {
                    log::error!("Resuming due to error: {:?}", e);
                    //TODO: Popup/info about the error? Or perhaps put the reason for the resume in the resume state below?
                    NetplayState::Resuming(self.resume())
                }
            }
        } else {
            //TODO: Actual input mapping..
            netplay_session.game_state.joypad_mapping =
                Some(if netplay_session.get_local_player_idx() == 0 {
                    JoypadMapping::P1
                } else {
                    JoypadMapping::P2
                });
            NetplayState::Connected(self)
        }
    }
}

impl Netplay<ResumingState> {
    fn advance(mut self) -> NetplayState {
        //log::trace!("Advancing Netplay<Resuming>");
        self.state.attempt2 = self.state.attempt2.advance();
        self.state.attempt1 = self.state.attempt1.advance();

        if let ConnectingState::Connected(_) = &self.state.attempt2 {
            NetplayState::Connecting(Netplay {
                state: self.state.attempt2,
            })
        } else if let ConnectingState::Connected(_) = &self.state.attempt1 {
            NetplayState::Connecting(Netplay {
                state: self.state.attempt1,
            })
        } else {
            NetplayState::Resuming(self)
        }
    }

    pub fn cancel(self) -> Netplay<LocalNesState> {
        log::debug!("Resume cancelled by user");
        self.disconnect()
    }
}

impl Netplay<FailedState> {
    fn advance(self) -> NetplayState {
        NetplayState::Failed(self)
    }
}
