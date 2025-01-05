use futures::channel::oneshot::Receiver;
use futures::{select, FutureExt};
use futures_timer::Delay;
use ggrs::{P2PSession, SessionBuilder, SessionState};
use matchbox_socket::{ChannelConfig, RtcIceServerConfig, WebRtcSocket, WebRtcSocketBuilder};

use serde::Deserialize;
use std::fmt::Debug;
use std::time::{Duration, Instant};

use crate::bundle::Bundle;
use crate::netplay::netplay_state::get_netplay_id;
use crate::settings::{Settings, MAX_PLAYERS};

use super::netplay_session::{GGRSConfig, NetplaySessionState};

use super::NetplayNesState;

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
    pub unlock_url: Option<String>,
}

pub enum ConnectingState {
    LoadingNetplayServerConfiguration(LoadingNetplayServerConfigurationState),
    PeeringUp(PeeringState),
    Synchronizing(SynchonizingState),

    //TODO: Get rid of this state?
    Connected(NetplaySessionState),

    Retrying(RetryingState),
    Failed(String),
}

impl ConnectingState {
    pub fn connect(start_method: StartMethod) -> Self {
        Self::start(start_method)
    }

    pub fn resume(
        game_state: NetplayNesState,
        session_id: String,
        netplay_server_configuration: StaticNetplayServerConfiguration,
    ) -> Self {
        Self::PeeringUp(PeeringState::new(
            netplay_server_configuration,
            StartMethod::Resume(StartState {
                game_state,
                session_id,
            }),
        ))
    }

    fn start(start_method: StartMethod) -> Self {
        let reqwest_client = reqwest::Client::new();
        match &Bundle::current().config.netplay.server {
            NetplayServerConfiguration::Static(conf) => {
                Self::PeeringUp(PeeringState::new(conf.clone(), start_method))
            }

            NetplayServerConfiguration::TurnOn(server) => {
                log::debug!("Fetching TurnOn config from server: {}", server);
                let netplay_id = get_netplay_id();
                let req = reqwest_client.get(format!("{server}/{netplay_id}")).send();
                let (sender, result) =
                    futures::channel::oneshot::channel::<Result<TurnOnResponse, TurnOnError>>();
                tokio::spawn(async move {
                    if let Err(e) = match req.await {
                        Ok(res) => {
                            log::trace!("Received response from TurnOn server: {:?}", res);
                            sender.send(res.json().await.map_err(|e| TurnOnError {
                                description: format!("Failed to receive response: {}", e),
                            }))
                        }
                        Err(e) => sender.send(Err(TurnOnError {
                            description: format!("Could not connect: {}", e),
                        })),
                    } {
                        log::error!("Could not send response: {:?}", e);
                    }
                });

                Self::LoadingNetplayServerConfiguration(LoadingNetplayServerConfigurationState {
                    result,
                    start_method,
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

pub struct LoadingNetplayServerConfigurationState {
    pub result: Receiver<Result<TurnOnResponse, TurnOnError>>,
    pub start_method: StartMethod,
}
impl LoadingNetplayServerConfigurationState {
    fn advance(mut self) -> ConnectingState {
        match &mut self.result.try_recv().map_err(|e| TurnOnError {
            description: format!("Unexpected error: {:?}", e),
        }) {
            Ok(Some(Ok(resp))) => {
                log::debug!("Got TurnOn config response: {:?}", resp);

                let netplay_server_configuration = match resp {
                    TurnOnResponse::Basic(BasicConfiguration { unlock_url, conf }) => {
                        conf.unlock_url = Some(unlock_url.to_string());
                        conf
                    }
                    TurnOnResponse::Full(conf) => conf,
                };
                ConnectingState::PeeringUp(PeeringState::new(
                    netplay_server_configuration.clone(),
                    self.start_method.clone(),
                ))
            }
            Ok(None) => ConnectingState::LoadingNetplayServerConfiguration(self), //No result yet
            Ok(Some(Err(e))) | Err(e) => {
                log::error!(
                    "Failed to retrieve netplay server configuration: {}, retrying...",
                    e.description
                );
                ConnectingState::Retrying(RetryingState::new(
                    format!(
                        "Failed to retrieve {} configuration.",
                        Bundle::current().config.vocabulary.netplay.name
                    ),
                    ConnectingState::start(self.start_method.clone()),
                    self.start_method,
                ))
            }
        }
    }
}

pub struct PeeringState {
    pub socket: WebRtcSocket,
    netplay_server_configuration: StaticNetplayServerConfiguration,
    pub start_method: StartMethod,
}
impl PeeringState {
    pub fn new(
        netplay_server_configuration: StaticNetplayServerConfiguration,
        start_method: StartMethod,
    ) -> Self {
        let matchbox_server = &netplay_server_configuration.matchbox.server;

        //TODO: matchbox will panic when we advance the frame on the ggrs session if we do not pass `players=2` here. See discussion (https://discord.com/channels/844211600009199626/1045611882691698688/1325596000928399495) for details.
        //      revert this when the bug is fixed in matchbox.
        let room_name = match &start_method {
            StartMethod::Start(StartState { session_id, .. }, ..) => {
                format!("join_{}?next=2", session_id)
            }
            StartMethod::Resume(StartState {
                session_id,
                game_state,
                ..
            }) => {
                format!("resume_{}_{}?next=2", session_id, game_state.frame)
            }
            StartMethod::MatchWithRandom(StartState { session_id, .. }) => {
                format!("random_{}?next=2", session_id)
            }
        };

        let (username, password) = match &netplay_server_configuration.matchbox.ice.credentials {
            IceCredentials::Password(IcePasswordCredentials { username, password }) => {
                (Some(username.to_string()), Some(password.to_string()))
            }
            IceCredentials::None => (None, None),
        };

        let (socket, loop_fut) = {
            let room_url = format!("ws://{matchbox_server}/{room_name}");
            let ice_server = RtcIceServerConfig {
                urls: netplay_server_configuration.matchbox.ice.urls.clone(),
                username,
                credential: password,
            };
            log::debug!(
                "Peering up through WebRTC socket: room_url={:?}, ice_server={:?}",
                room_url,
                ice_server
            );
            WebRtcSocketBuilder::new(room_url)
                .ice_server(ice_server)
                .add_channel(ChannelConfig::unreliable())
                .build()
        };

        let loop_fut = loop_fut.fuse();
        let timeout = Delay::new(Duration::from_millis(100));

        tokio::spawn(async move {
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
            netplay_server_configuration,
            start_method,
        }
    }

    fn advance(mut self) -> ConnectingState {
        let socket = &mut self.socket;
        socket.update_peers();

        let connected_peers = socket.connected_peers().count();
        if connected_peers >= MAX_PLAYERS {
            return ConnectingState::Failed("Room is full".to_string());
        }

        let remaining = MAX_PLAYERS - (connected_peers + 1);
        if remaining == 0 {
            log::debug!("Got all players! Synchonizing...");
            let players = socket.players();
            let ggrs_config = self.netplay_server_configuration.ggrs.clone();
            let mut sess_build = SessionBuilder::<GGRSConfig>::new()
                .with_num_players(MAX_PLAYERS)
                .with_input_delay(ggrs_config.input_delay)
                .with_fps(Settings::current_mut().get_nes_region().to_fps() as usize)
                .unwrap()
                .with_max_prediction_window(ggrs_config.max_prediction);

            for (i, player) in players.into_iter().enumerate() {
                sess_build = sess_build
                    .add_player(player, i)
                    .expect("player to be added to ggrs session");
            }

            ConnectingState::Synchronizing(SynchonizingState::new(
                sess_build
                    .start_p2p_session(socket.take_channel(0).expect("a channel"))
                    .expect("ggrs session to start"),
                self.start_method,
                self.netplay_server_configuration.clone(),
            ))
        } else {
            ConnectingState::PeeringUp(self)
        }
    }
}

pub struct SynchonizingState {
    p2p_session: P2PSession<GGRSConfig>,
    pub start_time: Instant,
    pub start_method: StartMethod,
    pub netplay_server_configuration: StaticNetplayServerConfiguration,
}
impl SynchonizingState {
    pub fn new(
        p2p_session: P2PSession<GGRSConfig>,
        start_method: StartMethod,
        netplay_server_configuration: StaticNetplayServerConfiguration,
    ) -> Self {
        SynchonizingState {
            p2p_session,
            start_time: Instant::now(),
            start_method,
            netplay_server_configuration,
        }
    }

    fn advance(mut self) -> ConnectingState {
        self.p2p_session.poll_remote_clients();
        if let SessionState::Running = self.p2p_session.current_state() {
            let start_method = self.start_method;
            log::debug!("Synchronized!");
            ConnectingState::Connected(NetplaySessionState::new(
                start_method.clone(),
                self.p2p_session,
                self.netplay_server_configuration,
            ))
        } else {
            ConnectingState::Synchronizing(self)
        }
    }
}
type RoomName = String;

#[derive(Clone, Debug)]
pub enum JoinOrHost {
    Join,
    Host,
}

#[derive(Clone, Debug)]
pub enum StartMethod {
    Start(StartState, RoomName, JoinOrHost),
    Resume(StartState),
    MatchWithRandom(StartState),
}

#[derive(Clone)]
pub struct StartState {
    pub game_state: NetplayNesState,
    pub session_id: String,
}

impl Debug for StartState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StartState")
            .field("session_id", &self.session_id)
            .finish()
    }
}

const RETRY_COOLDOWN: Duration = Duration::from_secs(5);
const MAX_RETRY_ATTEMPTS: u16 = 3;

pub struct RetryingState {
    failed_attempts: u16,
    pub deadline: Instant,
    pub fail_message: String,
    pub retry_state: Box<ConnectingState>, //The state we should resume to after the deadline
    start_method: StartMethod,
}
impl RetryingState {
    fn new(fail_message: String, retry_state: ConnectingState, start_method: StartMethod) -> Self {
        Self {
            failed_attempts: 1,
            deadline: Instant::now() + RETRY_COOLDOWN,
            fail_message,
            retry_state: Box::new(retry_state),
            start_method,
        }
    }

    fn advance(self) -> ConnectingState {
        if Instant::now().gt(&self.deadline) {
            match self.retry_state.advance() {
                ConnectingState::Retrying(mut retrying) => {
                    let failed_attempts = self.failed_attempts + 1;
                    if failed_attempts > MAX_RETRY_ATTEMPTS {
                        log::warn!("All retry attempt failed, using fallback configuration");
                        ConnectingState::PeeringUp(PeeringState::new(
                            StaticNetplayServerConfiguration {
                                matchbox: MatchboxConfiguration {
                                    server: "matchbox.netplay.tech:3536".to_string(),
                                    ice: IceConfiguration {
                                        urls: vec![
                                            "stun:stun.l.google.com:19302".to_string(),
                                            "stun:stun1.l.google.com:19302".to_string(),
                                        ],
                                        credentials: IceCredentials::None,
                                    },
                                },
                                ggrs: GGRSConfiguration {
                                    max_prediction: 12,
                                    input_delay: 2,
                                },
                                unlock_url: None,
                            },
                            self.start_method.clone(),
                        ))
                    } else {
                        log::info!(
                            "Retrying... ({}/{}) (Failure: {})",
                            failed_attempts,
                            MAX_RETRY_ATTEMPTS,
                            retrying.fail_message
                        );
                        retrying.failed_attempts = failed_attempts;
                        ConnectingState::Retrying(retrying)
                    }
                }
                other => other,
            }
        } else {
            ConnectingState::Retrying(self) //Keep waiting
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
