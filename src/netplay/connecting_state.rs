use futures::{FutureExt, select};
use futures_timer::Delay;
use ggrs::{P2PSession, SessionBuilder, SessionState};
use matchbox_socket::{ChannelConfig, RtcIceServerConfig, WebRtcSocketBuilder};

use serde::Deserialize;
use std::cell::RefCell;
use std::fmt::Debug;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::bundle::Bundle;
use crate::netplay::configuration::{
    GGRSConfiguration, IceConfiguration, MatchboxConfiguration, TurnOnServerConfiguration,
};
use crate::settings::{MAX_PLAYERS, Settings};

use super::netplay_session::{GGRSConfig, NetplaySessionState};

use super::NetplayNesState;
use super::configuration::{
    IceCredentials, IcePasswordCredentials, NetplayServerConfiguration,
    StaticNetplayServerConfiguration,
};

pub enum ConnectingState {
    Disconnected,
    LoadingNetplayServerConfiguration(StartMethod),
    PeeringUp(StartMethod),
    Synchronizing(SynchonizingState),

    //TODO: Get rid of this state?
    Connected(Option<NetplaySessionState>),

    Retrying,
    Failed(String),
}
pub type SharedConnectingState = Rc<RefCell<ConnectingState>>;
pub struct ConnectingSession {
    start_method: StartMethod,
    pub state: SharedConnectingState,
}

impl ConnectingSession {
    pub fn new(start_method: StartMethod) -> SharedConnectingState {
        let mut this = Self {
            start_method,
            state: Rc::new(RefCell::new(ConnectingState::Disconnected)),
        };
        let state = this.state.clone();
        println!("About to spawn local...");
        //TODO: drop this when this state is dropped
        tokio::task::spawn_local(async move {
            this.connect().await;
        });
        state
    }
    pub async fn connect(&mut self) {
        println!("Connecting!");
        let netplay_config = match &self.start_method {
            StartMethod::Resume(_, netplay_server_configuration) => {
                Ok(netplay_server_configuration.clone())
            }
            _ => match &Bundle::current().config.netplay.server {
                NetplayServerConfiguration::Static(static_conf) => Ok(static_conf.clone()),
                NetplayServerConfiguration::TurnOn(turn_on_conf) => {
                    self.load_netplay_server_configuration(turn_on_conf).await
                }
            },
        };
        let netplay_config = netplay_config.expect("TODO: Implement retry...");
        self.peer_up(netplay_config).await;
    }

    async fn load_netplay_server_configuration(
        &mut self,
        turn_on_conf: &TurnOnServerConfiguration,
    ) -> Result<StaticNetplayServerConfiguration, TurnOnError> {
        println!("Loading netplay configuration...");
        self.state
            .replace(ConnectingState::LoadingNetplayServerConfiguration(
                self.start_method.clone(),
            ));

        let netplay_id = turn_on_conf.get_netplay_id();
        let url = format!("{0}/{netplay_id}", turn_on_conf.url);
        log::debug!("Fetching TurnOn config from server: {}", url);

        let reqwest_client = reqwest::Client::new();
        let req = reqwest_client.get(url).send();
        //let state = self.state.clone();
        //tokio::task::spawn({
        //async move {
        let res = req.await.map_err(|e| TurnOnError {
            description: format!("Could not connect: {e}"),
        })?;

        log::debug!("Response from TurnOn server: {:?}", res);

        let result = if res.status().is_success() {
            res.json().await.map_err(|e| TurnOnError {
                description: format!("Failed to receive response: {e:?}"),
            })
        } else {
            Err(TurnOnError {
                description: format!("Response was not successful: {:?}", res.text().await),
            })
        };
        result.and_then(|mut resp| {
            log::debug!("Got TurnOn config response: {:?}", resp);

            let netplay_server_configuration = match &mut resp {
                TurnOnResponse::Basic(BasicConfiguration { unlock_url, conf }) => {
                    conf.unlock_url = Some(unlock_url.to_string());
                    conf
                }
                TurnOnResponse::Full(conf) => conf,
            };
            //*state.lock().unwrap() = Ted::PeeringUp;
            Ok(netplay_server_configuration.clone())
        })
        // match &mut result {
        //     Ok(resp) => {

        //     }
        //     Err(e) => {
        //         log::error!(
        //             "Failed to retrieve netplay server configuration: {}, retrying...",
        //             e.description
        //         );
        //         //*state.lock().unwrap() = Ted::Retrying;
        //         //TODO: Fix retry state
        //         // self.retry(format!(
        //         //     "Failed to retrieve {} configuration.",
        //         //     Bundle::current().config.vocabulary.netplay.name
        //         // ))
        //         // .await;
        //     }
        // }
        // Ok::<(), TurnOnError>(())
        //}
        //});
    }

    async fn peer_up(&mut self, netplay_server_configuration: StaticNetplayServerConfiguration) {
        println!("Peering up...");
        let matchbox_server = &netplay_server_configuration.matchbox.server;
        self.state
            .replace(ConnectingState::PeeringUp(self.start_method.clone()));
        //TODO: matchbox will panic when we advance the frame on the ggrs session if we do not pass `players=2` here. See discussion (https://discord.com/channels/844211600009199626/1045611882691698688/1325596000928399495) for details.
        //      revert this when the bug is fixed in matchbox.
        let room_name = match &self.start_method {
            StartMethod::Start(StartState { session_id, .. }, ..) => {
                format!("join_{}?next=2", session_id)
            }
            StartMethod::Resume(
                StartState {
                    session_id,
                    game_state,
                },
                ..,
            ) => {
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

        let (mut socket, loop_fut) = {
            let room_url = format!("ws://{matchbox_server}/{room_name}");
            let ice_server = RtcIceServerConfig {
                urls: netplay_server_configuration.matchbox.ice.urls.clone(),
                username,
                credential: password,
            };

            log::debug!(
                "Peering up through WebRTC socket:\nroom_url={room_url:?},\nice_server={ice_server:?}"
            );
            WebRtcSocketBuilder::new(room_url)
                .ice_server(ice_server)
                .add_channel(ChannelConfig::unreliable())
                .build()
        };

        let loop_fut = loop_fut.fuse();
        let timeout = Delay::new(Duration::from_millis(100));

        //tokio::task::spawn(async move {
        futures::pin_mut!(loop_fut, timeout);
        let ggrs_config = netplay_server_configuration.ggrs.clone();
        let res = loop {
            socket.update_peers();

            let connected_peers = socket.connected_peers().count();
            if connected_peers >= MAX_PLAYERS {
                break Err("Room is Full".to_string());
            }

            let remaining = MAX_PLAYERS - (connected_peers + 1);
            if remaining <= 0 {
                break Ok(());
            }
            select! {
                _ = (&mut timeout).fuse() => {
                    timeout.reset(Duration::from_millis(100));
                }

                _ = &mut loop_fut => {
                    break Err("TODO".to_string());
                }
            }
        };

        match res {
            Ok(_) => {
                log::debug!("Got all players! Synchonizing...");
                let players = socket.players();

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
                self.synchronize(
                    sess_build
                        .start_p2p_session(socket.take_channel(0).expect("a channel"))
                        .expect("ggrs session to start"),
                    netplay_server_configuration.clone(),
                )
                .await;
            }
            Err(_) => {
                //TODO: Set state failed
                //ConnectingState::Failed("Room is full".to_string());
            }
        }
        //});
    }

    async fn synchronize(
        &mut self,
        mut p2p_session: P2PSession<GGRSConfig>,
        netplay_server_configuration: StaticNetplayServerConfiguration,
    ) {
        self.state
            .replace(ConnectingState::Synchronizing(SynchonizingState::new(
                self.start_method.clone(),
                netplay_server_configuration.clone(),
            )));

        let mut ticker = tokio::time::interval(Duration::from_millis(100));
        let res = loop {
            p2p_session.poll_remote_clients();
            //TODO: What about fail state? Implement retry?
            if let SessionState::Running = p2p_session.current_state() {
                log::debug!("Synchronized!");
                break NetplaySessionState::new(
                    self.start_method.clone(),
                    p2p_session,
                    netplay_server_configuration,
                );
            }
            ticker.tick().await;
        };
        self.state.replace(ConnectingState::Connected(Some(res)));
    }

    async fn retry(&self, fail_message: String) {
        todo!()
    }
}

pub struct SynchonizingState {
    pub start_time: Instant,
    pub start_method: StartMethod,
    pub netplay_server_configuration: StaticNetplayServerConfiguration,
}
impl SynchonizingState {
    pub fn new(
        start_method: StartMethod,
        netplay_server_configuration: StaticNetplayServerConfiguration,
    ) -> Self {
        SynchonizingState {
            start_time: Instant::now(),
            start_method,
            netplay_server_configuration,
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
    Resume(StartState, StaticNetplayServerConfiguration),
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

//TODO: Implement retry state
// const RETRY_COOLDOWN: Duration = Duration::from_secs(5);
// const MAX_RETRY_ATTEMPTS: u16 = 3;

// pub struct RetryingState {
//     failed_attempts: u16,
//     pub deadline: Instant,
//     pub fail_message: String,
//     pub retry_state: Box<ConnectingState>, //The state we should resume to after the deadline
//     start_method: StartMethod,
// }
// impl RetryingState {
//     fn new(fail_message: String, retry_state: ConnectingState, start_method: StartMethod) -> Self {
//         Self {
//             failed_attempts: 1,
//             deadline: Instant::now() + RETRY_COOLDOWN,
//             fail_message,
//             retry_state: Box::new(retry_state),
//             start_method,
//         }
//     }

//     fn advance(self) -> ConnectingState {
//         if Instant::now().gt(&self.deadline) {
//             match self.retry_state.advance() {
//                 ConnectingState::Retrying(mut retrying) => {
//                     let failed_attempts = self.failed_attempts + 1;
//                     if failed_attempts > MAX_RETRY_ATTEMPTS {
//                         log::warn!("All retry attempt failed, using fallback configuration");
//                         ConnectingState::PeeringUp(PeeringState::new(
//                             StaticNetplayServerConfiguration {
//                                 matchbox: MatchboxConfiguration {
//                                     server: "matchbox.netplay.tech:3536".to_string(),
//                                     ice: IceConfiguration {
//                                         urls: vec![
//                                             "stun:stun.l.google.com:19302".to_string(),
//                                             "stun:stun1.l.google.com:19302".to_string(),
//                                         ],
//                                         credentials: IceCredentials::None,
//                                     },
//                                 },
//                                 ggrs: GGRSConfiguration {
//                                     max_prediction: 12,
//                                     input_delay: 2,
//                                 },
//                                 unlock_url: None,
//                             },
//                             self.start_method.clone(),
//                         ))
//                     } else {
//                         log::info!(
//                             "Retrying... ({}/{}) (Failure: {})",
//                             failed_attempts,
//                             MAX_RETRY_ATTEMPTS,
//                             retrying.fail_message
//                         );
//                         retrying.failed_attempts = failed_attempts;
//                         ConnectingState::Retrying(retrying)
//                     }
//                 }
//                 other => other,
//             }
//         } else {
//             ConnectingState::Retrying(self) //Keep waiting
//         }
//     }
// }

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
