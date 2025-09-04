use anyhow::Result;
use futures::{FutureExt, pin_mut, select};
use futures_timer::Delay;
use matchbox_socket::{ChannelConfig, RtcIceServerConfig, WebRtcSocket, WebRtcSocketBuilder};

use serde::Deserialize;
use std::fmt::Debug;
use std::pin::Pin;
use std::time::{Duration, Instant};
use tokio::sync::watch::{Receiver, Sender, channel};

use crate::bundle::Bundle;
use crate::netplay::configuration::TurnOnServerConfiguration;
use crate::netplay::session::NetplayNesState;
use crate::settings::MAX_PLAYERS;

use super::configuration::{
    IceCredentials, IcePasswordCredentials, NetplayServerConfiguration,
    StaticNetplayServerConfiguration,
};

pub enum ConnectingState {
    Idle, //Initial state.
    LoadingNetplayServerConfiguration,
    PeeringUp(
        StartMethod,
        Option<String>, /* Unlock url */
        Instant,        /* Start time */
    ),
}

//TODO: Some kind of type state pattern for the ConnectingSession where only certain methods are available per state and where a shared version of that state is also propagated.
//      Probably same for NetplaySession (or maybe only for NetplaySession?? Probably for both...)
pub struct NetplayConnection {
    pub socket: WebRtcSocket,
    pub netplay_server_configuration: StaticNetplayServerConfiguration,
    pub initial_state: Option<NetplayNesState>,
}
pub struct ConnectingSession {
    pub state: Receiver<ConnectingState>,
    pub netplay_connection: Pin<Box<dyn Future<Output = Result<NetplayConnection>>>>,
}

impl ConnectingSession {
    pub fn connect(start_method: StartMethod) -> Self {
        let (state_sender, state) = channel(ConnectingState::Idle);

        let netplay_connection = async move {
            let netplay_config = match &start_method {
                StartMethod::Resume(netplay_server_configuration, ..) => {
                    netplay_server_configuration.clone()
                }
                _ => match &Bundle::current().config.netplay.server {
                    NetplayServerConfiguration::Static(static_conf) => static_conf.clone(),
                    NetplayServerConfiguration::TurnOn(turn_on_conf) => {
                        ConnectingSession::retry(|| {
                            ConnectingSession::load_netplay_server_configuration(
                                state_sender.clone(),
                                turn_on_conf,
                            )
                        })
                        .await
                        .unwrap_or_default()
                    }
                },
            };

            ConnectingSession::peer_up(state_sender, start_method, netplay_config).await
        };
        Self {
            state,
            netplay_connection: Box::pin(netplay_connection),
        }
    }

    async fn load_netplay_server_configuration(
        state_sender: Sender<ConnectingState>,
        turn_on_conf: &TurnOnServerConfiguration,
    ) -> Result<StaticNetplayServerConfiguration, TurnOnError> {
        let _ = state_sender.send(ConnectingState::LoadingNetplayServerConfiguration);

        let netplay_id = turn_on_conf.get_netplay_id();
        let url = format!("{0}/{netplay_id}", turn_on_conf.url);
        log::debug!("Fetching TurnOn config from server: {}", url);

        let reqwest_client = reqwest::Client::new();
        let res = reqwest_client
            .get(url)
            .send()
            .await
            .map_err(|e| TurnOnError {
                description: format!("Could not connect: {e}"),
            })?;

        log::debug!("Response from TurnOn server: {:?}", res);

        if res.status().is_success() {
            res.json().await.map_err(|e| TurnOnError {
                description: format!("Failed to receive response: {e:?}"),
            })
        } else {
            Err(TurnOnError {
                description: format!("Response was not successful: {:?}", res.text().await),
            })
        }
        .and_then(|mut resp| {
            log::debug!("Got TurnOn config response: {:?}", resp);

            let netplay_server_configuration = match &mut resp {
                TurnOnResponse::Basic(BasicConfiguration { unlock_url, conf }) => {
                    conf.unlock_url = Some(unlock_url.to_string());
                    conf
                }
                TurnOnResponse::Full(conf) => conf,
            };
            Ok(netplay_server_configuration.clone())
        })
    }

    async fn peer_up(
        state_sender: Sender<ConnectingState>,
        start_method: StartMethod,
        netplay_server_configuration: StaticNetplayServerConfiguration,
    ) -> Result<NetplayConnection> {
        let matchbox_server = &netplay_server_configuration.matchbox.server;
        let _ = state_sender.send(ConnectingState::PeeringUp(
            start_method.clone(),
            netplay_server_configuration.unlock_url.clone(),
            Instant::now(),
        ));

        let room_name = start_method.to_room_name();

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

        tokio::task::spawn(async move {
            let loop_fut = loop_fut.fuse();
            let timeout = Delay::new(Duration::from_millis(100));
            pin_mut!(loop_fut, timeout);
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
            println!("BROKE THE LOOP!!");
        });

        let res = loop {
            socket.update_peers();

            let connected_peers = socket.connected_peers().count();
            //dbg!(connected_peers);
            if connected_peers >= MAX_PLAYERS {
                break Err("Room is Full".to_string());
            }

            let remaining = MAX_PLAYERS - (connected_peers + 1);
            if remaining <= 0 {
                break Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        };

        match res {
            Ok(_) => {
                log::debug!("Got all players!");

                let initial_state = match &start_method {
                    StartMethod::Resume(_, resumed_state) => {
                        let mut state = resumed_state.clone();
                        //ggrs will start over from 0
                        state.ggrs_frame = 0;

                        Some(state)
                    }
                    _ => None,
                };

                Ok(NetplayConnection {
                    socket,
                    netplay_server_configuration,
                    initial_state,
                })
            }
            Err(_) => anyhow::bail!("TODO"),
        }
    }

    pub async fn retry<T, E, Fut, F>(mut task: F) -> Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: Debug,
    {
        const RETRY_COOLDOWN: Duration = Duration::from_secs(5);
        const MAX_RETRY_ATTEMPTS: u16 = 3;

        let mut failed_attempts = 0_u16;
        loop {
            match task().await {
                Ok(t) => break Ok(t),
                Err(e) => {
                    failed_attempts += 1;
                    log::warn!(
                        "Failed in retry ({failed_attempts}/{MAX_RETRY_ATTEMPTS}) (Failure: {e:?})"
                    );
                    if failed_attempts >= MAX_RETRY_ATTEMPTS {
                        log::error!("All retry attempt failed, bailing");
                        break Err(e);
                    } else {
                        log::info!("Retrying in {RETRY_COOLDOWN:?}...");
                        tokio::time::sleep(RETRY_COOLDOWN).await
                    }
                }
            }
        }
    }
}

type RoomName = String;

#[derive(Clone, Debug)]
pub enum JoinOrHost {
    Join,
    Host,
}

#[derive(Clone)]
pub enum StartMethod {
    Start(RoomName, JoinOrHost),
    Resume(StaticNetplayServerConfiguration, NetplayNesState),
    MatchWithRandom,
}

impl StartMethod {
    pub fn to_room_name(&self) -> String {
        let netplay_rom = &Bundle::current().netplay_rom;
        let rom_hash = format!("{:x}", md5::compute(netplay_rom));

        let room_name = match &self {
            StartMethod::Start(room_name, ..) => {
                let room_name = room_name.to_uppercase();
                format!("join_{room_name}_{rom_hash}")
            }
            StartMethod::Resume(_, state_snapshot) => {
                // TODO: When resuming using this room there might be collisions, but it's unlikely.
                //       Should be fixed though.

                format!("resume_{rom_hash}_{}", state_snapshot.ggrs_frame)
            }
            StartMethod::MatchWithRandom => {
                format!("random_{rom_hash}")
            }
        };

        //TODO: matchbox will panic when we advance the frame on the ggrs session if we do not pass `next=2` here. See discussion (https://discord.com/channels/844211600009199626/1045611882691698688/1325596000928399495) for details.
        //      revert this when the bug is fixed in matchbox.
        format!("{room_name}?next=2")
    }
}

#[derive(Deserialize, Debug)]
pub struct TurnOnError {
    #[allow(dead_code)] // Will be read by the Debug trait (And that's what it's for)
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
