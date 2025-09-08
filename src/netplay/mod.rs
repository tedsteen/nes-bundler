use std::{
    sync::{Arc, RwLock},
    time::Instant,
};

use tokio::sync::watch::channel;

use self::connection::StartMethod;

use crate::{
    emulation::{LocalNesState, NESBuffers, NesStateHandler},
    input::JoypadState,
    netplay::{
        connection::{ConnectingState, JoinOrHost},
        session::NetplaySession,
    },
    settings::MAX_PLAYERS,
};

pub mod configuration;
pub mod connection;
pub mod gui;
pub mod session;

#[cfg(feature = "debug")]
mod stats;

#[derive(Clone, Debug)]
pub enum NetplayCommand {
    JoinGame(String),
    FindGame,
    HostGame,

    CancelConnect,
    RetryConnect,

    #[cfg(feature = "debug")] // Only used to fake disconnects
    Resume,
    Disconnect,
}
pub type NetplayCommandBus = tokio::sync::mpsc::Sender<NetplayCommand>;
pub enum SharedNetplayConnectedState {
    Synchronizing,
    Running(Instant /* Start time */),
}
pub enum SharedNetplayState {
    Disconnected,
    Connecting(tokio::sync::watch::Receiver<ConnectingState>),
    Connected(SharedNetplayConnectedState),
    Resuming,
    Failed(String),
}

#[derive(Clone)]
pub struct SharedNetplay {
    pub command_tx: NetplayCommandBus,
    pub receiver: tokio::sync::watch::Receiver<SharedNetplayState>,
    pub sender: tokio::sync::watch::Sender<SharedNetplayState>,
    pub command_rx: Arc<RwLock<Option<tokio::sync::mpsc::Receiver<NetplayCommand>>>>,

    #[cfg(feature = "debug")]
    pub stats: Arc<RwLock<[crate::netplay::stats::NetplayStats; crate::settings::MAX_PLAYERS]>>,
}
impl SharedNetplay {
    pub fn new() -> Self {
        let (command_tx, command_rx) = tokio::sync::mpsc::channel(1);
        let (sender, receiver) = channel(SharedNetplayState::Disconnected);
        Self {
            command_tx,
            command_rx: Arc::new(RwLock::new(Some(command_rx))),
            receiver,
            sender,

            #[cfg(feature = "debug")]
            stats: Arc::new(RwLock::new([
                crate::netplay::stats::NetplayStats::new(),
                crate::netplay::stats::NetplayStats::new(),
            ])),
        }
    }
}

pub const MAX_ROOM_NAME_LEN: u8 = 4;

pub struct Netplay {
    shared_state_sender: tokio::sync::watch::Sender<SharedNetplayState>,
    session: NetplaySession,
    netplay_rx: tokio::sync::mpsc::Receiver<NetplayCommand>,
    initial_local_nes_state: LocalNesState,
    #[cfg(feature = "debug")]
    stats: Arc<RwLock<[stats::NetplayStats; 2]>>,
}

impl Netplay {
    pub fn new(local_play_nes_state: LocalNesState, shared_netplay: SharedNetplay) -> Self {
        Self {
            initial_local_nes_state: local_play_nes_state.clone(),
            session: NetplaySession::new(local_play_nes_state),
            shared_state_sender: shared_netplay.sender,
            netplay_rx: shared_netplay.command_rx.write().unwrap().take().unwrap(),
            #[cfg(feature = "debug")]
            stats: shared_netplay.stats.clone(),
        }
    }

    fn start(&mut self, start_method: StartMethod) {
        match &mut self.session {
            NetplaySession::Disconnected(..) => {
                self.session = NetplaySession::start(start_method);
            }
            state => {
                log::warn!("Ignored start command in state {state:?}");
            }
        }
    }

    fn disconnect(&mut self) {
        self.session = NetplaySession::new(self.initial_local_nes_state.clone());
    }
}

impl NesStateHandler for Netplay {
    async fn advance(
        &mut self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        buffers: &mut NESBuffers<'_>,
    ) {
        // drain pending netplay commands
        #[cfg(feature = "netplay")]
        while let Ok(cmd) = self.netplay_rx.try_recv() {
            use crate::netplay::{
                connection::ConnectingSession,
                session::{ConnectingNetplaySession, FailedNetplaySession},
            };

            match cmd {
                NetplayCommand::FindGame => {
                    self.start(StartMethod::MatchWithRandom);
                }

                NetplayCommand::HostGame => {
                    use rand::distr::{Alphanumeric, SampleString};
                    let room_name = Alphanumeric
                        .sample_string(&mut rand::rng(), MAX_ROOM_NAME_LEN.into())
                        .to_uppercase();
                    self.start(StartMethod::Start(room_name, JoinOrHost::Host));
                }

                NetplayCommand::JoinGame(room_name) => {
                    self.start(StartMethod::Start(room_name.to_string(), JoinOrHost::Join));
                }

                NetplayCommand::CancelConnect => {
                    self.disconnect();
                }

                NetplayCommand::RetryConnect => match &mut self.session {
                    NetplaySession::Connecting(ConnectingNetplaySession {
                        connecting_session: ConnectingSession { start_method, .. },
                        ..
                    })
                    | NetplaySession::Failed(FailedNetplaySession { start_method, .. }) => {
                        self.session = NetplaySession::start(start_method.clone());
                    }
                    state => {
                        log::warn!("Ignored retry command in state {state:?}");
                    }
                },

                #[cfg(feature = "debug")] //Only used to fake disconnects
                NetplayCommand::Resume => match &mut self.session {
                    NetplaySession::Connected(s) => {
                        log::debug!("Manually resuming connection (faking a lost connection)");
                        self.session = NetplaySession::resume(s);
                    }
                    state => {
                        log::warn!("Ignored resume command in state {state:?}");
                    }
                },

                NetplayCommand::Disconnect => {
                    self.disconnect();
                }
            }
        }
        self.session.advance(joypad_state, buffers).await;

        #[cfg(feature = "debug")]
        if let NetplaySession::Connected(connected_netplay_session) = &self.session {
            if connected_netplay_session.current_game_state.ggrs_frame % 30 == 0 {
                let sess = &connected_netplay_session.p2p_session;
                puffin::profile_scope!("Netplay stats");
                for i in 0..MAX_PLAYERS {
                    if let Ok(stats) = sess.network_stats(i) {
                        if !sess.local_player_handles().contains(&i) {
                            self.stats.write().unwrap()[i].push_stats(stats);
                        }
                    }
                }
            };
        }

        let _ = self
            .shared_state_sender
            .send(self.session.to_shared_state());
    }

    fn reset(&mut self, hard: bool) {
        self.session.reset(hard);
    }

    fn set_speed(&mut self, speed: f32) {
        self.session.set_speed(speed);
    }

    fn get_samples_per_frame(&self) -> f32 {
        self.session.get_samples_per_frame()
    }

    fn save_sram(&self) -> Option<&[u8]> {
        self.session.save_sram()
    }

    fn frame(&self) -> u32 {
        self.session.frame()
    }
}
