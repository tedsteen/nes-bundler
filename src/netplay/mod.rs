use crate::{input::JoypadInput, settings::{MAX_PLAYERS, Settings}, Fps, MyGameState, FPS};
use futures::{select, FutureExt};
use futures_timer::Delay;
use ggrs::{Config, GGRSRequest, NetworkStats, P2PSession, SessionBuilder};
use matchbox_socket::{WebRtcSocket, WebRtcSocketConfig, RtcIceServerConfig, RtcIceCredentials, RtcIcePasswordCredentials};
use rusticnes_core::nes::NesState;
use serde::Deserialize;
use uuid::Uuid;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use tokio::{runtime::Runtime, time::sleep};

use self::state::{StartMethod, ConnectedState, InputMapping, ConnectingState};
pub use self::state::NetplayState;
pub mod state;

impl Clone for MyGameState {
    fn clone(&self) -> Self {
        let data = &mut self.save();
        let mut clone = Self {
            nes: NesState::new(self.nes.mapper.clone()),
        };
        clone.load(data);
        clone
    }
}

#[derive(Debug)]
pub struct GGRSConfig;
impl Config for GGRSConfig {
    type Input = u8;
    type State = MyGameState;
    type Address = String;
}
pub const STATS_HISTORY: usize = 100;

pub struct NetplayStat {
    pub stat: NetworkStats,
    pub duration: Duration,
}
pub struct NetplayStats {
    stats: VecDeque<NetplayStat>,
    start_time: Instant,
}

impl NetplayStats {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            stats: VecDeque::with_capacity(STATS_HISTORY),
        }
    }

    pub fn get_ping(&self) -> &VecDeque<NetplayStat> {
        &self.stats
    }

    fn push_stats(&mut self, stat: NetworkStats) {
        let duration = Instant::now().duration_since(self.start_time);
        self.stats.push_back(NetplayStat { duration, stat });
        if self.stats.len() == STATS_HISTORY {
            self.stats.pop_front();
        }
    }
}

pub enum NetplaySessionState {
    //Some peers are disconnected
    DisconnectedPeers,
    Connected
}
pub struct NetplaySession {
    p2p_session: P2PSession<GGRSConfig>,
    frame: Frame,
    last_confirmed_frame: Frame,
    pub stats: [NetplayStats; MAX_PLAYERS],
    state: NetplaySessionState,
    requested_fps: Fps,
}

impl NetplaySession {
    pub fn new(p2p_session: P2PSession<GGRSConfig>) -> Self {
        Self {
            p2p_session,
            frame: 0,
            last_confirmed_frame: -1,
            stats: [NetplayStats::new(), NetplayStats::new()],
            state: NetplaySessionState::Connected,
            requested_fps: FPS,
        }
    }

    pub fn advance(&mut self, game_state: &mut MyGameState, inputs: [JoypadInput; MAX_PLAYERS]) {

        let sess = &mut self.p2p_session;
        let frame = &mut self.frame;
        sess.poll_remote_clients();

        for event in sess.events() {
            if let ggrs::GGRSEvent::Disconnected { addr } = event {
                eprintln!("Lost peer {}, disconnecting...", addr);
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
                        GGRSRequest::LoadGameState {
                            cell,
                            frame: load_state_frame,
                        } => {
                            println!("Loading (frame {:?})", load_state_frame);
                            *game_state = cell.load().expect("No data found.");
                            *frame = load_state_frame;
                        }
                        GGRSRequest::SaveGameState {
                            cell,
                            frame: save_state_frame,
                        } => {
                            assert_eq!(*frame, save_state_frame);
                            cell.save(*frame, Some(game_state.clone()), None);
                        }
                        GGRSRequest::AdvanceFrame { inputs } => {
                            game_state.advance([
                                JoypadInput(inputs[0].0),
                                JoypadInput(inputs[1].0),
                            ]);

                            if *frame <= self.last_confirmed_frame {
                                // Discard the samples for this frame since it's a replay from ggrs. Audio has already been produced and pushed for it.
                                game_state.nes.apu.consume_samples();
                            } else {
                                self.last_confirmed_frame = *frame;
                            }
                            *frame += 1;
                        }
                    }
                }
            }
            Err(ggrs::GGRSError::PredictionThreshold) => {
                println!("Frame {} skipped: PredictionThreshold", frame);
            }
            Err(ggrs::GGRSError::NotSynchronized) => {
            }
            Err(e) => eprintln!("Ouch :( {:?}", e),
        }

        if *frame % 30 == 0 {
            for i in 0..MAX_PLAYERS {
                if let Ok(stats) = sess.network_stats(i as usize) {
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

type Frame = i32;
pub struct Netplay {
    rt: Runtime,
    pub state: NetplayState,
    pub config: NetplayBuildConfiguration,
    pub room_name: String,
}

#[derive(Deserialize, Clone)]
pub struct NetplayBuildConfiguration {
    pub default_room_name: String,
    pub netplay_id: Option<String>,
    pub server: NetplayServerConfiguration,
}

#[derive(Deserialize, Clone)]
pub struct IcePasswordCredentials {
    username: String,
    password: String,
}
#[derive(Deserialize, Clone)]
pub enum IceCredentials {
    None,
    Password(IcePasswordCredentials)
}
#[derive(Deserialize, Clone)]
pub struct IceConfiguration {
    urls: Vec<String>,
    credentials: IceCredentials,
}

#[derive(Deserialize, Clone)]
pub struct MatchboxConfiguration {
    server: String,
    ice: IceConfiguration,
}

#[derive(Deserialize, Clone)]
pub struct GGRSConfiguration {
    pub max_prediction: usize,
    pub input_delay: usize,
}
#[derive(Deserialize, Clone)]
pub struct StaticNetplayServerConfiguration {
    matchbox: MatchboxConfiguration,
    pub ggrs: GGRSConfiguration,
}

#[derive(Deserialize, Clone)]
pub enum NetplayServerConfiguration {
    Static(StaticNetplayServerConfiguration),
}

impl Netplay {
    pub fn new(config: &NetplayBuildConfiguration) -> Self {
        let room_name = config.default_room_name.clone();
        Netplay {
            rt: Runtime::new().expect("Could not create an async runtime"),
            state: NetplayState::Disconnected,
            config: config.clone(),
            room_name,
        }
    }

    pub fn get_netplay_id(&self, settings: &mut Settings) -> String {
        self.config.netplay_id.as_ref().unwrap_or_else(|| {
            settings.netplay_id.get_or_insert_with(|| Uuid::new_v4().to_string())
        }).clone()
    }

    pub fn start(&mut self, start_method: StartMethod) {
        let promise = match &self.config.server {
            NetplayServerConfiguration::Static(conf) => {
                let conf = conf.clone();
                self.rt.spawn(async move {
                    sleep(Duration::from_secs(0)).await;
                    conf
                 })
            }
        };
        self.state = NetplayState::Connecting(start_method, ConnectingState::LoadingNetplayServerConfiguration(promise));
    }

    pub fn advance(
        &mut self,
        game_state: &mut MyGameState,
        inputs: [JoypadInput; MAX_PLAYERS],
    ) -> Fps {
        if let Some(new_state) = match &mut self.state {
            NetplayState::Disconnected => {
                game_state.advance(inputs);
                None
            }
            NetplayState::Connecting(start_method, connecting_state) => {
                match connecting_state {
                    ConnectingState::LoadingNetplayServerConfiguration(conf) => {
                        game_state.advance(inputs);
                        let is_finished = conf.is_finished();
                        if is_finished {
                            if let Some(Ok(conf)) = conf.now_or_never() {
                                let matchbox_server = &conf.matchbox.server;
                                let credentials = match &conf.matchbox.ice.credentials {
                                    IceCredentials::Password(IcePasswordCredentials { username, password }) => RtcIceCredentials::Password(RtcIcePasswordCredentials { username: username.to_string(), password: password.to_string() }),
                                    IceCredentials::None => RtcIceCredentials::None,
                                };
                                let room = match &start_method {
                                    state::StartMethod::Create(name) => name.clone(),
                                    state::StartMethod::Random => "beta-0?next=2".to_string(),
                                };

                                let (socket, loop_fut) = WebRtcSocket::new_with_config(WebRtcSocketConfig {
                                    room_url: format!("ws://{matchbox_server}/{room}"),
                                    ice_server: RtcIceServerConfig {
                                        urls: conf.matchbox.ice.urls.clone(),
                                        credentials
                                    },
                                });

                                let loop_fut = loop_fut.fuse();
                                self.rt.spawn(async move {
                                    futures::pin_mut!(loop_fut);

                                    let timeout = Delay::new(Duration::from_millis(100));
                                    futures::pin_mut!(timeout);

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
                                *connecting_state = ConnectingState::PeeringUp(Some(socket), conf.ggrs.clone());
                            }
                        }
                        None
                    }
                    ConnectingState::PeeringUp(maybe_socket, ggrs_config) => {
                        let mut new_state = None;
                        game_state.advance(inputs);

                        if let Some(socket) = maybe_socket {
                            socket.accept_new_connections();
                            let connected_peers = socket.connected_peers().len();
                            let remaining = MAX_PLAYERS - (connected_peers + 1);
                            if remaining == 0 {
                                let players = socket.players();
                                let ggrs_config = ggrs_config;
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

                                new_state = Some(NetplayState::Connected(
                                    NetplaySession::new(sess_build.start_p2p_session(maybe_socket.take().unwrap()).unwrap()), ConnectedState::MappingInput));
                                game_state.nes.reset();
                            }
                        }
                        new_state
                    }
                }
            }
            NetplayState::Connected(netplay_session, connected_state) => {
                match connected_state {
                    state::ConnectedState::MappingInput => {
                        netplay_session.advance(game_state, inputs);
                        //TODO: Actual input mapping..
                        *connected_state = ConnectedState::Playing(InputMapping { ids: [0, 1] });
                    }
                    state::ConnectedState::Playing(_input_mapping) => {
                        netplay_session.advance(game_state, inputs);
                    }
                }
                
                if let NetplaySessionState::DisconnectedPeers = netplay_session.state {
                    // For now, just disconnect if we loose peers
                    self.state = NetplayState::Disconnected;
                }
                None
            }
        } {
            self.state = new_state;
        }
        
        if let NetplayState::Connected(netplay_session, _) = &self.state {
            netplay_session.requested_fps
        } else {
            FPS
        }
    }
}
