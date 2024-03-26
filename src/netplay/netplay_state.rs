use uuid::Uuid;

use crate::{
    bundle::Bundle,
    input::JoypadState,
    nes_state::{FrameData, LocalNesState, NesStateHandler},
    settings::{Settings, MAX_PLAYERS},
    window::NESFrame,
};

use super::{
    netplay_session::NetplaySession, ConnectingState, JoypadMapping, StartMethod, StartState,
};

pub enum NetplayState {
    Disconnected(Box<Netplay<LocalNesState>>),
    Connecting(Netplay<ConnectingState>),
    Connected(Box<Netplay<Connected>>),
    Resuming(Netplay<Resuming>),
    Failed(Box<Netplay<Failed>>),
}

pub struct Failed {
    pub reason: String,
}

impl NetplayState {
    pub fn advance(
        self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        nes_frame: &mut Option<&mut NESFrame>,
    ) -> (Self, Option<FrameData>) {
        use NetplayState::*;
        match self {
            Connecting(netplay) => {
                if let Some(nes_frame) = nes_frame {
                    nes_frame.fill(0); //Black screen while connecting
                }
                netplay.advance()
            }
            Connected(netplay) => netplay.advance(joypad_state, nes_frame),
            Resuming(netplay) => {
                if let Some(nes_frame) = nes_frame {
                    nes_frame.fill(0); //Black screen while resuming
                }
                netplay.advance()
            }
            Disconnected(netplay) => netplay.advance(joypad_state, nes_frame),
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

    pub fn disconnect(self) -> Box<Netplay<LocalNesState>> {
        log::debug!("Disconnecting");
        Box::new(Netplay::new())
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
    Settings::current()
        .netplay_id
        .get_or_insert_with(|| Uuid::new_v4().to_string())
        .to_string()
}
impl Netplay<LocalNesState> {
    pub fn new() -> Self {
        Self {
            state: LocalNesState::load_rom(&Bundle::current().rom),
        }
    }

    pub fn join_by_name(self, room_name: &str) -> NetplayState {
        let netplay_rom = &Bundle::current().netplay_rom;
        let session_id = format!("{}_{:x}", room_name, md5::compute(netplay_rom));
        let nes_state = LocalNesState::load_rom(netplay_rom);
        self.join(StartMethod::Join(
            StartState {
                game_state: super::NetplayNesState::new(nes_state),
                session_id,
            },
            room_name.to_string(),
        ))
    }

    pub fn match_with_random(self) -> NetplayState {
        let netplay_rom = &Bundle::current().netplay_rom;
        let rom_hash = md5::compute(netplay_rom);

        // TODO: When resuming using this session id there might be collisions, but it's unlikely.
        //       Should be fixed though.
        let session_id = format!("{:x}", rom_hash);
        let nes_state = LocalNesState::load_rom(netplay_rom);
        self.join(StartMethod::MatchWithRandom(StartState {
            game_state: super::NetplayNesState::new(nes_state),
            session_id,
        }))
    }

    pub fn join(self, start_method: StartMethod) -> NetplayState {
        log::debug!("Joining: {:?}", start_method);
        NetplayState::Connecting(Netplay::from(ConnectingState::connect(start_method)))
    }
    fn advance(
        mut self,
        joypad_state: [JoypadState; 2],
        nes_frame: &mut Option<&mut NESFrame>,
    ) -> (NetplayState, Option<FrameData>) {
        let frame_data = self.state.advance(joypad_state, nes_frame);
        (NetplayState::Disconnected(Box::new(self)), frame_data)
    }
}

impl Netplay<ConnectingState> {
    pub fn cancel(self) -> Box<Netplay<LocalNesState>> {
        log::debug!("Connection cancelled by user");
        self.disconnect()
    }

    fn advance(mut self) -> (NetplayState, Option<FrameData>) {
        //log::trace!("Advancing Netplay<ConnectingState>");
        self.state = self.state.advance();
        (
            match self.state {
                ConnectingState::Connected(connected) => {
                    log::debug!("Connected! Starting netplay session");
                    NetplayState::Connected(Box::new(Netplay {
                        state: Connected {
                            netplay_session: connected.state,
                            session_id: match connected.start_method {
                                StartMethod::Join(StartState { session_id, .. }, _)
                                | StartMethod::MatchWithRandom(StartState { session_id, .. })
                                | StartMethod::Resume(StartState { session_id, .. }) => session_id,
                            },
                        },
                    }))
                }
                ConnectingState::Failed(reason) => NetplayState::Failed(Box::new(Netplay {
                    state: Failed { reason },
                })),
                _ => NetplayState::Connecting(self),
            },
            None,
        )
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
        nes_frame: &mut Option<&mut NESFrame>,
    ) -> (NetplayState, Option<FrameData>) {
        //log::trace!("Advancing Netplay<Connected>");
        let netplay_session = &mut self.state.netplay_session;

        if let Some(joypad_mapping) = &mut netplay_session.game_state.joypad_mapping.clone() {
            match netplay_session.advance(joypad_state, joypad_mapping, nes_frame) {
                Ok(frame_data) => (NetplayState::Connected(Box::new(self)), frame_data),
                Err(e) => {
                    log::error!("Resuming due to error: {:?}", e);
                    //TODO: Popup/info about the error? Or perhaps put the reason for the resume in the resume state below?
                    (NetplayState::Resuming(self.resume()), None)
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
            (NetplayState::Connected(Box::new(self)), None)
        }
    }
}

impl Netplay<Resuming> {
    fn advance(mut self) -> (NetplayState, Option<FrameData>) {
        //log::trace!("Advancing Netplay<Resuming>");
        self.state.attempt1 = self.state.attempt1.advance();
        self.state.attempt2 = self.state.attempt2.advance();

        (
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
            },
            None,
        )
    }

    pub fn cancel(self) -> Box<Netplay<LocalNesState>> {
        log::debug!("Resume cancelled by user");
        self.disconnect()
    }
}

impl Netplay<Failed> {
    pub fn restart(self) -> Box<Netplay<LocalNesState>> {
        self.disconnect()
    }

    fn advance(self) -> (NetplayState, Option<FrameData>) {
        (NetplayState::Failed(Box::new(self)), None)
    }
}
