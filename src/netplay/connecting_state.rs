use futures::channel::oneshot::Receiver;
use futures::{select, FutureExt};
use futures_timer::Delay;
use ggrs::{P2PSession, SessionBuilder, SessionState};
use matchbox_socket::{ChannelConfig, RtcIceServerConfig, WebRtcSocket, WebRtcSocketBuilder};
use md5::Digest;
use serde::Deserialize;
use std::rc::Rc;
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
    Retrying(Connecting<Retrying>),
}

impl ConnectingState {
    pub fn connect<T>(netplay: &Netplay<T>, start_method: StartMethod) -> Self {
        Self::start(
            netplay.config.server.clone(),
            Rc::clone(&netplay.rt),
            netplay.netplay_id.clone(),
            netplay.rom_hash,
            start_method,
        )
    }

    fn start(
        netplay_server_config: NetplayServerConfiguration,
        rt: Rc<Runtime>,
        netplay_id: String,
        rom_hash: Digest,
        start_method: StartMethod,
    ) -> Self {
        let reqwest_client = reqwest::Client::new();

        match &netplay_server_config {
            NetplayServerConfiguration::Static(conf) => {
                Self::PeeringUp(Connecting::<PeeringState>::new(
                    netplay_server_config.clone(),
                    conf.clone(),
                    rt,
                    netplay_id,
                    rom_hash,
                    start_method,
                ))
            }

            NetplayServerConfiguration::TurnOn(server) => {
                let req = reqwest_client.get(format!("{server}/{netplay_id}")).send();
                let (sender, result) =
                    futures::channel::oneshot::channel::<Result<TurnOnResponse, TurnOnError>>();
                rt.spawn(async move {
                    let _ = match req.await {
                        Ok(res) => sender.send(res.json().await.map_err(|e| TurnOnError {
                            description: format!("Failed to receive response: {}", e),
                        })),
                        Err(e) => sender.send(Err(TurnOnError {
                            description: format!("Could not connect: {}", e),
                        })),
                    };
                });
                Self::LoadingNetplayServerConfiguration(Connecting {
                    rt,
                    start_method,
                    netplay_server_config,
                    netplay_id,
                    rom_hash,
                    state: LoadingNetplayServerConfiguration { result },
                })
            }
        }
    }

    pub fn advance(self) -> ConnectingState {
        match self {
            ConnectingState::LoadingNetplayServerConfiguration(loading) => loading.advance(),
            ConnectingState::PeeringUp(peering) => peering.advance(),
            ConnectingState::Synchronizing(synchronizing) => synchronizing.advance(),
            ConnectingState::Retrying(retrying) => retrying.advance(),
            _ => self,
        }
    }
}
pub struct Connecting<S> {
    rt: Rc<Runtime>,
    netplay_server_config: NetplayServerConfiguration,
    netplay_id: String,
    rom_hash: Digest,
    pub start_method: StartMethod,
    pub state: S,
}

impl<T> Connecting<T> {
    fn from<S>(state: T, other: Connecting<S>) -> Self {
        Self {
            rt: other.rt,
            netplay_server_config: other.netplay_server_config,
            netplay_id: other.netplay_id,
            rom_hash: other.rom_hash,
            start_method: other.start_method,
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
        rt: &Rc<Runtime>,
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
            StartMethod::Create(_, name) => format!("join_{:x}_{}", rom_hash, name),
            StartMethod::Resume(StartState { game_state, .. }) => {
                format!("resume_{:x}", md5::compute(game_state.save()))
            }
            StartMethod::Random(_) => format!("random_{:x}?next=2", rom_hash),
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
        SynchonizingState {
            p2p_session,
            unlock_url,
            start_time: Instant::now(),
        }
    }
}

#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
pub enum StartMethod {
    Create(StartState, String),
    Resume(StartState),
    Random(StartState),
}

#[derive(Clone)]
pub struct StartState {
    pub input_mapping: Option<InputMapping>,
    pub game_state: LocalGameState,
}

impl Connecting<LoadingNetplayServerConfiguration> {
    fn into_retrying(self, fail_message: String) -> Connecting<Retrying> {
        Connecting::from(
            Retrying::new(
                fail_message,
                ConnectingState::start(
                    self.netplay_server_config.clone(),
                    self.rt.clone(),
                    self.netplay_id.clone(),
                    self.rom_hash,
                    self.start_method.clone(),
                ),
            ),
            self,
        )
    }

    fn advance(mut self) -> ConnectingState {
        match self.state.result.try_recv() {
            Ok(Some(Ok(resp))) => ConnectingState::PeeringUp(Connecting::from(
                PeeringState::new(&self.rt, resp, self.start_method.clone(), &self.rom_hash),
                self,
            )),
            Ok(None) => ConnectingState::LoadingNetplayServerConfiguration(self), //No result yet
            Ok(Some(Err(err))) => ConnectingState::Retrying(
                self.into_retrying(format!("Could not fetch server config ({:?})", err)),
            ),
            //Lost the sender, not much to do but fail
            Err(_) => ConnectingState::Retrying(self.into_retrying("Unexpected error".to_string())),
        }
    }
}

impl Connecting<PeeringState> {
    fn new(
        netplay_server_config: NetplayServerConfiguration,
        conf: StaticNetplayServerConfiguration,
        rt: Rc<Runtime>,
        netplay_id: String,
        rom_hash: Digest,
        start_method: StartMethod,
    ) -> Self {
        Self {
            state: PeeringState::new(
                &rt,
                TurnOnResponse::Full(conf.clone()),
                start_method.clone(),
                &rom_hash,
            ),
            rt,
            start_method,
            netplay_server_config,
            netplay_id,
            rom_hash,
        }
    }

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
                .expect("Could not start session");

            for (i, player) in players.into_iter().enumerate() {
                sess_build = sess_build
                    .add_player(player, i)
                    .expect("failed to add player");
            }

            ConnectingState::Synchronizing(Connecting {
                rt: self.rt,
                netplay_server_config: self.netplay_server_config,
                netplay_id: self.netplay_id,
                rom_hash: self.rom_hash,
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

            ConnectingState::Connected(Connecting {
                rt: self.rt,
                netplay_server_config: self.netplay_server_config,
                netplay_id: self.netplay_id,
                rom_hash: self.rom_hash,
                start_method: start_method.clone(),
                state: NetplaySession::new(start_method.clone(), self.state.p2p_session),
            })
        } else {
            ConnectingState::Synchronizing(self)
        }
    }
}

pub struct Retrying {
    pub deadline: Instant,
    pub fail_message: String,
    pub retry_state: Box<ConnectingState>, //The state we should resume to after the deadline
}
impl Retrying {
    fn new(fail_message: String, retry_state: ConnectingState) -> Self {
        Self {
            deadline: Instant::now() + Duration::from_secs(5),
            fail_message,
            retry_state: Box::new(retry_state),
        }
    }
}

impl Connecting<Retrying> {
    fn advance(self) -> ConnectingState {
        if Instant::now().gt(&self.state.deadline) {
            *self.state.retry_state
        } else {
            ConnectingState::Retrying(self)
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
