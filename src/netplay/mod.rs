use crate::{input::JoypadInput, settings::MAX_PLAYERS, Fps, LocalGameState, FPS};
use futures::channel::oneshot::Receiver;
use futures::{select, FutureExt};
use futures_timer::Delay;
use ggrs::{Config, GGRSRequest, P2PSession};
use matchbox_socket::{
    ChannelConfig, PeerId, RtcIceServerConfig, WebRtcSocket, WebRtcSocketBuilder,
};
use md5::Digest;
use serde::Deserialize;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use uuid::Uuid;

use self::stats::NetplayStats;

pub mod gui;
pub mod state_handler;
mod stats;

#[derive(Debug)]
pub struct GGRSConfig;
impl Config for GGRSConfig {
    type Input = u8;
    type State = LocalGameState;
    type Address = PeerId;
}

pub struct InputMapping {
    pub ids: [usize; MAX_PLAYERS],
}

#[derive(Clone, Debug)]
pub enum StartMethod {
    Create(String),
    //Resume(SavedNetplaySession),
    Random,
}

pub enum ConnectedState {
    //Mapping netplay input
    MappingInput,
    //Playing
    Playing(InputMapping),
}

#[derive(Deserialize, Debug)]
pub struct TurnOnError {
    pub description: String,
}

pub struct PeeringState {
    pub socket: Option<WebRtcSocket>,
    pub ggrs_config: GGRSConfiguration,
    pub unlock_url: Option<String>,
}
impl PeeringState {
    pub fn new(
        socket: Option<WebRtcSocket>,
        ggrs_config: GGRSConfiguration,
        unlock_url: Option<String>,
    ) -> Self {
        PeeringState {
            socket,
            ggrs_config,
            unlock_url,
        }
    }
}

pub struct SynchonizingState {
    pub p2p_session: Option<P2PSession<GGRSConfig>>,
    pub unlock_url: Option<String>,
    pub start_time: Instant,
}
impl SynchonizingState {
    pub fn new(p2p_session: Option<P2PSession<GGRSConfig>>, unlock_url: Option<String>) -> Self {
        let start_time = Instant::now();
        SynchonizingState {
            p2p_session,
            unlock_url,
            start_time,
        }
    }
}

#[allow(clippy::large_enum_variant)]
pub enum ConnectingState {
    //Load a server config
    LoadingNetplayServerConfiguration(Receiver<Result<TurnOnResponse, TurnOnError>>),
    //Connecting all peers
    PeeringUp(PeeringState),
    Synchronizing(SynchonizingState),
}

#[allow(clippy::large_enum_variant)]
pub enum NetplayState {
    Disconnected,
    Connecting(StartMethod, ConnectingState),
    Connected(NetplaySession, ConnectedState),
}

pub enum NetplaySessionState {
    //Some peers are disconnected
    DisconnectedPeers,
    Connected,
}
pub struct NetplaySession {
    p2p_session: P2PSession<GGRSConfig>,
    last_confirmed_frame: i32,
    pub stats: [NetplayStats; MAX_PLAYERS],
    state: NetplaySessionState,
    requested_fps: Fps,
}

impl NetplaySession {
    pub fn new(p2p_session: P2PSession<GGRSConfig>) -> Self {
        Self {
            p2p_session,
            last_confirmed_frame: -1,
            stats: [NetplayStats::new(), NetplayStats::new()],
            state: NetplaySessionState::Connected,
            requested_fps: FPS,
        }
    }

    pub fn advance(&mut self, game_state: &mut LocalGameState, inputs: [JoypadInput; MAX_PLAYERS]) {
        let sess = &mut self.p2p_session;
        sess.poll_remote_clients();

        for event in sess.events() {
            if let ggrs::GGRSEvent::Disconnected { addr } = event {
                eprintln!("Lost peer {:?}, disconnecting...", addr);
                self.state = NetplaySessionState::DisconnectedPeers;
                return;
            }
        }

        for handle in sess.local_player_handles() {
            let local_input = 0;
            sess.add_local_input(handle, inputs[local_input].0).unwrap();
        }

        match sess.advance_frame() {
            Ok(requests) => {
                for request in requests {
                    match request {
                        GGRSRequest::LoadGameState { cell, frame } => {
                            println!("Loading (frame {:?})", frame);
                            *game_state = cell.load().expect("No data found.");
                        }
                        GGRSRequest::SaveGameState { cell, frame } => {
                            assert_eq!(game_state.frame, frame);
                            cell.save(frame, Some(game_state.clone()), None);
                        }
                        GGRSRequest::AdvanceFrame { inputs } => {
                            game_state
                                .advance([JoypadInput(inputs[0].0), JoypadInput(inputs[1].0)]);

                            if game_state.frame <= self.last_confirmed_frame {
                                // Discard the samples for this frame since it's a replay from ggrs. Audio has already been produced and pushed for it.
                                game_state.nes.apu.consume_samples();
                            } else {
                                self.last_confirmed_frame = game_state.frame;
                            }
                        }
                    }
                }
            }
            Err(ggrs::GGRSError::PredictionThreshold) => {
                println!("Frame {} skipped: PredictionThreshold", game_state.frame);
            }
            Err(ggrs::GGRSError::NotSynchronized) => {}
            Err(e) => eprintln!("Ouch :( {:?}", e),
        }

        if game_state.frame % 30 == 0 {
            for i in 0..MAX_PLAYERS {
                if let Ok(stats) = sess.network_stats(i) {
                    if !sess.local_player_handles().contains(&i) {
                        self.stats[i].push_stats(stats);
                    }
                }
            }
        }
        if sess.frames_ahead() > 0 {
            self.requested_fps = (FPS as f32 * 0.9) as u32;
        } else {
            self.requested_fps = FPS
        }
    }
}
pub struct Netplay {
    rt: Runtime,
    pub state: NetplayState,
    pub config: NetplayBuildConfiguration,
    reqwest_client: reqwest::Client,
    netplay_id: String,
    rom_hash: Digest,
}

impl Netplay {
    pub fn new(
        config: NetplayBuildConfiguration,
        netplay_id: &mut Option<String>,
        rom_hash: Digest,
    ) -> Self {
        Self {
            rt: Runtime::new().expect("Could not create an async runtime for Netplay"),
            state: NetplayState::Disconnected,
            config,
            reqwest_client: reqwest::Client::new(),
            netplay_id: netplay_id
                .get_or_insert_with(|| Uuid::new_v4().to_string())
                .to_string(),
            rom_hash,
        }
    }

    pub fn start(&mut self, start_method: StartMethod) {
        match &self.config.server {
            NetplayServerConfiguration::Static(conf) => {
                self.state = NetplayState::Connecting(
                    start_method.clone(),
                    self.start_peering(TurnOnResponse::Full(conf.clone()), start_method),
                );
            }
            NetplayServerConfiguration::TurnOn(server) => {
                let netplay_id = &self.netplay_id;
                let req = self
                    .reqwest_client
                    .get(format!("{server}/{netplay_id}"))
                    .send();
                let (sender, receiver) =
                    futures::channel::oneshot::channel::<Result<TurnOnResponse, TurnOnError>>();
                self.rt.spawn(async move {
                    let _ = match req.await {
                        Ok(res) => sender.send(res.json().await.map_err(|e| TurnOnError {
                            description: format!("Failed to receive response: {}", e),
                        })),
                        Err(e) => sender.send(Err(TurnOnError {
                            description: format!("Could not connect: {}", e),
                        })),
                    };
                });
                self.state = NetplayState::Connecting(
                    start_method,
                    ConnectingState::LoadingNetplayServerConfiguration(receiver),
                );
            }
        };
    }

    fn start_peering(&self, resp: TurnOnResponse, start_method: StartMethod) -> ConnectingState {
        let mut maybe_unlock_url = None;
        let conf = match resp {
            TurnOnResponse::Basic(BasicResponse { unlock_url, conf }) => {
                maybe_unlock_url = Some(unlock_url);
                conf
            }
            TurnOnResponse::Full(conf) => conf,
        };
        let matchbox_server = &conf.matchbox.server;

        let room = match &start_method {
            StartMethod::Create(name) => {
                format!("join_{:x}_{}", self.rom_hash, name.clone())
            }
            //state::StartMethod::Resume(old_session) => format!("resume_{game_hash}_{}", old_session.name.clone()),
            StartMethod::Random => format!("random_{:x}?next=2", self.rom_hash),
        };

        let (username, password) = match &conf.matchbox.ice.credentials {
            IceCredentials::Password(IcePasswordCredentials { username, password }) => {
                (Some(username.to_string()), Some(password.to_string()))
            }
            IceCredentials::None => (None, None),
        };

        let (socket, loop_fut) = WebRtcSocketBuilder::new(format!("ws://{matchbox_server}/{room}"))
            .ice_server(RtcIceServerConfig {
                urls: conf.matchbox.ice.urls.clone(),
                username,
                credential: password,
            })
            .add_channel(ChannelConfig::unreliable())
            .build();

        let loop_fut = loop_fut.fuse();

        self.rt.spawn(async move {
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

        ConnectingState::PeeringUp(PeeringState::new(
            Some(socket),
            conf.ggrs.clone(),
            maybe_unlock_url,
        ))
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct NetplayBuildConfiguration {
    pub default_room_name: String,
    pub netplay_id: Option<String>,
    pub server: NetplayServerConfiguration,
}

#[derive(Deserialize, Clone, Debug)]
pub struct IcePasswordCredentials {
    username: String,
    password: String,
}
#[derive(Deserialize, Clone, Debug)]
pub enum IceCredentials {
    None,
    Password(IcePasswordCredentials),
}
#[derive(Deserialize, Clone, Debug)]
pub struct IceConfiguration {
    urls: Vec<String>,
    credentials: IceCredentials,
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
pub struct StaticNetplayServerConfiguration {
    matchbox: MatchboxConfiguration,
    pub ggrs: GGRSConfiguration,
}

#[derive(Deserialize, Debug)]
pub struct BasicResponse {
    unlock_url: String,
    conf: StaticNetplayServerConfiguration,
}

#[derive(Deserialize, Debug)]
pub enum TurnOnResponse {
    Basic(BasicResponse),
    Full(StaticNetplayServerConfiguration),
}

#[derive(Deserialize, Clone, Debug)]
pub enum NetplayServerConfiguration {
    Static(StaticNetplayServerConfiguration),
    //An external server for fetching TURN credentials
    TurnOn(String),
}
