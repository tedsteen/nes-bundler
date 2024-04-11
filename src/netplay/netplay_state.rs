use anyhow::Result;
use uuid::Uuid;

use crate::{
    bundle::Bundle,
    emulation::{LocalNesState, NESBuffers, NesStateHandler},
    input::JoypadState,
    settings::{Settings, MAX_PLAYERS},
};

use super::{
    connecting_state::JoinOrHost, netplay_session::NetplaySession, ConnectingState, JoypadMapping,
    StartMethod, StartState,
};

pub enum NetplayState {
    Disconnected(Netplay<LocalNesState>),
    Connecting(Netplay<ConnectingState>),
    Connected(Netplay<Connected>),
    Resuming(Netplay<Resuming>),
    Failed(Netplay<Failed>),
}

pub struct Failed {
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
                //Black screen and no sound while connecting
                if let Some(video) = &mut buffers.video {
                    video.fill(0);
                }
                if let Some(audio) = &mut buffers.audio {
                    for _ in 0..1000 {
                        audio.push(0.0);
                    }
                }

                netplay.advance()
            }
            Connected(netplay) => netplay.advance(joypad_state, buffers),
            Resuming(netplay) => {
                if let Some(video) = &mut buffers.video {
                    video.fill(0);
                }
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
            attempt1: ConnectingState::connect(StartMethod::Resume(StartState {
                game_state: netplay_session.last_confirmed_game_states[1].clone(),
                session_id: session_id.clone(),
            })),
            attempt2: ConnectingState::connect(StartMethod::Resume(StartState {
                game_state: netplay_session.last_confirmed_game_states[0].clone(),
                session_id,
            })),
        }
    }
}
pub fn get_netplay_id() -> String {
    Settings::current_mut()
        .netplay_id
        .get_or_insert_with(|| Uuid::new_v4().to_string())
        .to_string()
}
impl Netplay<LocalNesState> {
    pub fn new() -> Result<Self> {
        Ok(Self {
            state: LocalNesState::start_rom(&Bundle::current().rom, true)?,
        })
    }

    pub fn host_game(self) -> Result<NetplayState> {
        use rand::distributions::{Alphanumeric, DistString};

        let room_name = Alphanumeric
            .sample_string(&mut rand::thread_rng(), 5)
            .to_uppercase();

        self.join_or_host(&room_name, JoinOrHost::Host)
    }

    pub fn join_game(self, room_name: &str) -> Result<NetplayState> {
        self.join_or_host(room_name, JoinOrHost::Join)
    }

    fn join_or_host(self, room_name: &str, join_or_host: JoinOrHost) -> Result<NetplayState> {
        let netplay_rom = &Bundle::current().netplay_rom;
        let session_id = format!("{}_{:x}", room_name, md5::compute(netplay_rom));
        let nes_state = LocalNesState::start_rom(netplay_rom, false)?;
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
        let nes_state = LocalNesState::start_rom(netplay_rom, false)?;
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
                    state: Connected {
                        netplay_session: connected.state,
                        session_id: match connected.start_method {
                            StartMethod::Start(StartState { session_id, .. }, ..)
                            | StartMethod::MatchWithRandom(StartState { session_id, .. })
                            | StartMethod::Resume(StartState { session_id, .. }) => session_id,
                        },
                    },
                })
            }
            ConnectingState::Failed(reason) => NetplayState::Failed(Netplay {
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

        Netplay::from(Resuming::new(&mut self))
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

impl Netplay<Resuming> {
    fn advance(mut self) -> NetplayState {
        //log::trace!("Advancing Netplay<Resuming>");
        self.state.attempt1 = self.state.attempt1.advance();
        self.state.attempt2 = self.state.attempt2.advance();

        if let ConnectingState::Connected(_) = &self.state.attempt1 {
            NetplayState::Connecting(Netplay {
                state: self.state.attempt1,
            })
        } else if let ConnectingState::Connected(_) = &self.state.attempt2 {
            NetplayState::Connecting(Netplay {
                state: self.state.attempt2,
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

impl Netplay<Failed> {
    pub fn restart(self) -> Netplay<LocalNesState> {
        self.disconnect()
    }

    fn advance(self) -> NetplayState {
        NetplayState::Failed(self)
    }
}
