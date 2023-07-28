use futures::channel::oneshot::Receiver;
use futures::{select, FutureExt};
use futures_timer::Delay;
use ggrs::{P2PSession, SessionBuilder, SessionState};
use matchbox_socket::{ChannelConfig, RtcIceServerConfig, WebRtcSocket, WebRtcSocketBuilder};
use md5::Digest;
use serde::Deserialize;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

use crate::{settings::MAX_PLAYERS, LocalGameState, FPS};

use super::netplay_session::{GGRSConfig, NetplaySession};
use super::netplay_state::Netplay;
use super::InputMapping;

#[derive(Deserialize, Clone, Debug)]
pub enum NetplayServerConfiguration {
    Static(StaticNetplayServerConfiguration),
    //An external server for fetching TURN credentials
    TurnOn(String),
}

#[derive(Deserialize, Clone, Debug)]
pub struct StaticNetplayServerConfiguration {
    matchbox: MatchboxConfiguration,
    pub ggrs: GGRSConfiguration,
}

#[allow(clippy::large_enum_variant)]
pub enum ConnectingState {
    LoadingNetplayServerConfiguration(Connecting<LoadingNetplayServerConfiguration>),
    PeeringUp(Connecting<PeeringState>),
    Synchronizing(Connecting<SynchonizingState>),
    Connected(Connecting<NetplaySession>),
    Failed(String),
}

impl ConnectingState {
    pub fn advance(self, rt: &mut Runtime, rom_hash: &Digest) -> ConnectingState {
        match self {
            ConnectingState::LoadingNetplayServerConfiguration(state) => {
                state.advance(rt, rom_hash)
            }
            ConnectingState::PeeringUp(state) => state.advance(),
            ConnectingState::Synchronizing(state) => state.advance(),
            ConnectingState::Connected(_) => self,
            ConnectingState::Failed(_) => self,
        }
    }
}
pub struct Connecting<S> {
    pub initial_game_state: LocalGameState,
    pub start_method: StartMethod,
    pub state: S,
}

impl<T> Connecting<T> {
    fn from<S>(state: T, other: Connecting<S>) -> Self {
        Self {
            start_method: other.start_method,
            initial_game_state: other.initial_game_state,
            state,
        }
    }
}

pub struct LoadingNetplayServerConfiguration {
    pub result: Receiver<Result<TurnOnResponse, TurnOnError>>,
}

pub struct PeeringState {
    pub socket: WebRtcSocket,
    ggrs_config: GGRSConfiguration,
    unlock_url: Option<String>,
}
impl PeeringState {
    pub fn new(
        rt: &mut Runtime,
        resp: TurnOnResponse,
        start_method: StartMethod,
        rom_hash: &Digest,
    ) -> Self {
        let mut maybe_unlock_url = None;
        let conf = match resp {
            TurnOnResponse::Basic(BasicConfiguration { unlock_url, conf }) => {
                maybe_unlock_url = Some(unlock_url);
                conf
            }
            TurnOnResponse::Full(conf) => conf,
        };
        let matchbox_server = &conf.matchbox.server;

        let room_name = match &start_method {
            StartMethod::Create(name) => format!("join_{:x}_{}", rom_hash, name),
            StartMethod::Resume(ResumableNetplaySession { game_state, .. }) => {
                format!("resume_{:x}", md5::compute(game_state.save()))
            }
            StartMethod::Random => format!("random_{:x}?next=2", rom_hash),
        };

        let (username, password) = match &conf.matchbox.ice.credentials {
            IceCredentials::Password(IcePasswordCredentials { username, password }) => {
                (Some(username.to_string()), Some(password.to_string()))
            }
            IceCredentials::None => (None, None),
        };

        let (socket, loop_fut) =
            WebRtcSocketBuilder::new(format!("ws://{matchbox_server}/{room_name}"))
                .ice_server(RtcIceServerConfig {
                    urls: conf.matchbox.ice.urls.clone(),
                    username,
                    credential: password,
                })
                .add_channel(ChannelConfig::unreliable())
                .build();

        let loop_fut = loop_fut.fuse();

        rt.spawn(async move {
            let timeout = Delay::new(Duration::from_millis(100));
            futures::pin_mut!(loop_fut, timeout);
            loop {
                select! {
                    _ = (&mut timeout).fuse() => {
                        timeout.reset(Duration::from_millis(100));
                    }

                    _ = &mut loop_fut => {
                        break;
                    }
                }
            }
        });

        Self {
            socket,
            ggrs_config: conf.ggrs.clone(),
            unlock_url: maybe_unlock_url,
        }
    }
}

pub struct SynchonizingState {
    p2p_session: P2PSession<GGRSConfig>,
    pub unlock_url: Option<String>,
    pub start_time: Instant,
}
impl SynchonizingState {
    pub fn new(p2p_session: P2PSession<GGRSConfig>, unlock_url: Option<String>) -> Self {
        let start_time = Instant::now();
        SynchonizingState {
            p2p_session,
            unlock_url,
            start_time,
        }
    }
}

#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
pub enum StartMethod {
    Create(String),
    Resume(ResumableNetplaySession),
    Random,
}

#[derive(Clone)]
pub struct ResumableNetplaySession {
    pub input_mapping: Option<InputMapping>,
    pub game_state: LocalGameState,
}

impl ResumableNetplaySession {
    pub fn new(input_mapping: Option<InputMapping>, game_state: LocalGameState) -> Self {
        Self {
            input_mapping,
            game_state,
        }
    }
}
impl Connecting<LoadingNetplayServerConfiguration> {
    pub fn create<T>(netplay: &mut Netplay<T>, start_method: StartMethod) -> ConnectingState {
        let reqwest_client = reqwest::Client::new();
        let netplay_id = netplay.netplay_id.clone();

        match &netplay.config.server {
            NetplayServerConfiguration::Static(conf) => ConnectingState::PeeringUp(Connecting {
                initial_game_state: netplay.initial_game_state.clone(),
                start_method: start_method.clone(),
                state: PeeringState::new(
                    &mut netplay.rt,
                    TurnOnResponse::Full(conf.clone()),
                    start_method,
                    &netplay.rom_hash,
                ),
            }),

            NetplayServerConfiguration::TurnOn(server) => {
                let req = reqwest_client.get(format!("{server}/{netplay_id}")).send();
                let (sender, result) =
                    futures::channel::oneshot::channel::<Result<TurnOnResponse, TurnOnError>>();
                netplay.rt.spawn(async move {
                    let _ = match req.await {
                        Ok(res) => sender.send(res.json().await.map_err(|e| TurnOnError {
                            description: format!("Failed to receive response: {}", e),
                        })),
                        Err(e) => sender.send(Err(TurnOnError {
                            description: format!("Could not connect: {}", e),
                        })),
                    };
                });
                ConnectingState::LoadingNetplayServerConfiguration(Connecting {
                    initial_game_state: netplay.initial_game_state.clone(),
                    start_method,
                    state: LoadingNetplayServerConfiguration { result },
                })
            }
        }
    }

    fn advance(mut self, rt: &mut Runtime, rom_hash: &Digest) -> ConnectingState {
        match self.state.result.try_recv() {
            Ok(Some(Ok(resp))) => ConnectingState::PeeringUp(Connecting::from(
                PeeringState::new(rt, resp, self.start_method.clone(), rom_hash),
                self,
            )),
            Ok(None) => ConnectingState::LoadingNetplayServerConfiguration(self), //No result yet
            Ok(Some(Err(err))) => {
                //TODO: alert about not being able to fetch server configuration
                ConnectingState::Failed(format!("Could not fetch server config :( {:?}", err))
            }
            Err(_) => {
                //Lost the sender, not much to do but fail
                ConnectingState::Failed("Unexpected error".to_string())
            }
        }
    }
}
impl Connecting<PeeringState> {
    fn advance(mut self) -> ConnectingState {
        let socket = &mut self.state.socket;
        socket.update_peers();

        let connected_peers = socket.connected_peers().count();
        let remaining = MAX_PLAYERS - (connected_peers + 1);
        if remaining == 0 {
            let players = socket.players();
            let ggrs_config = self.state.ggrs_config.clone();
            let mut sess_build = SessionBuilder::<GGRSConfig>::new()
                .with_num_players(MAX_PLAYERS)
                .with_max_prediction_window(ggrs_config.max_prediction)
                .with_input_delay(ggrs_config.input_delay)
                .with_fps(FPS as usize)
                .expect("invalid fps");

            for (i, player) in players.into_iter().enumerate() {
                sess_build = sess_build
                    .add_player(player, i)
                    .expect("failed to add player");
            }

            ConnectingState::Synchronizing(Connecting {
                initial_game_state: self.initial_game_state,
                start_method: self.start_method,
                state: SynchonizingState::new(
                    sess_build
                        .start_p2p_session(self.state.socket)
                        .expect("p2p session should be able to start"),
                    self.state.unlock_url.clone(),
                ),
            })
        } else {
            ConnectingState::PeeringUp(self)
        }
    }
}

impl Connecting<SynchonizingState> {
    fn advance(mut self) -> ConnectingState {
        self.state.p2p_session.poll_remote_clients();
        if let SessionState::Running = self.state.p2p_session.current_state() {
            let start_method = self.start_method;
            let initial_game_state = self.initial_game_state.clone();

            ConnectingState::Connected(Connecting {
                initial_game_state: self.initial_game_state,
                start_method: start_method.clone(),
                state: NetplaySession::new(
                    start_method.clone(),
                    self.state.p2p_session,
                    initial_game_state,
                ),
            })
        } else {
            ConnectingState::Synchronizing(self)
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct TurnOnError {
    pub description: String,
}

#[derive(Deserialize, Debug)]
pub enum TurnOnResponse {
    Basic(BasicConfiguration),
    Full(StaticNetplayServerConfiguration),
}

#[derive(Deserialize, Debug)]
pub struct BasicConfiguration {
    unlock_url: String,
    conf: StaticNetplayServerConfiguration,
}

#[derive(Deserialize, Clone, Debug)]
pub struct IcePasswordCredentials {
    username: String,
    password: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct MatchboxConfiguration {
    server: String,
    ice: IceConfiguration,
}

#[derive(Deserialize, Clone, Debug)]
pub struct GGRSConfiguration {
    pub max_prediction: usize,
    pub input_delay: usize,
}

#[derive(Deserialize, Clone, Debug)]
pub struct IceConfiguration {
    urls: Vec<String>,
    credentials: IceCredentials,
}

#[derive(Deserialize, Clone, Debug)]
pub enum IceCredentials {
    None,
    Password(IcePasswordCredentials),
}
