use crate::{input::JoypadInput, settings::MAX_PLAYERS, Fps, LocalGameState, FPS};
use futures::channel::oneshot::Receiver;
use futures::{select, FutureExt};
use futures_timer::Delay;
use ggrs::{Config, GGRSRequest, P2PSession, SessionBuilder, SessionState};
use matchbox_socket::{
    ChannelConfig, PeerId, RtcIceServerConfig, WebRtcSocket, WebRtcSocketBuilder,
};
use md5::Digest;
use serde::Deserialize;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use uuid::Uuid;

pub mod gui;
pub mod state_handler;
#[cfg(feature = "debug")]
mod stats;

#[derive(Debug)]
pub struct GGRSConfig;
impl Config for GGRSConfig {
    type Input = u8;
    type State = LocalGameState;
    type Address = PeerId;
}

#[derive(Clone, Debug)]
pub struct InputMapping {
    pub ids: [usize; MAX_PLAYERS],
}
impl InputMapping {
    fn map(&self, local_input: usize) -> usize {
        self.ids[local_input]
    }
}

#[derive(Clone)]
pub struct ResumableNetplaySession {
    input_mapping: Option<InputMapping>,
    game_state: LocalGameState,
}

impl ResumableNetplaySession {
    fn new(input_mapping: Option<InputMapping>, game_state: LocalGameState) -> Self {
        Self {
            input_mapping,
            game_state,
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

#[derive(Deserialize, Debug)]
pub struct TurnOnError {
    pub description: String,
}

pub struct PeeringState {
    start_method: StartMethod,
    socket: Option<WebRtcSocket>,
    ggrs_config: GGRSConfiguration,
    unlock_url: Option<String>,
}
impl PeeringState {
    pub fn new(
        start_method: StartMethod,
        socket: Option<WebRtcSocket>,
        ggrs_config: GGRSConfiguration,
        unlock_url: Option<String>,
    ) -> Self {
        PeeringState {
            start_method,
            socket,
            ggrs_config,
            unlock_url,
        }
    }
}

pub struct SynchonizingState {
    start_method: StartMethod,
    p2p_session: Option<P2PSession<GGRSConfig>>,
    unlock_url: Option<String>,
    start_time: Instant,
}
impl SynchonizingState {
    pub fn new(
        start_method: StartMethod,
        p2p_session: Option<P2PSession<GGRSConfig>>,
        unlock_url: Option<String>,
    ) -> Self {
        let start_time = Instant::now();
        SynchonizingState {
            start_method,
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
    Connected(NetplaySession),
    Disconnected,
}

impl ConnectingState {
    pub fn new_peering_up(
        rt: &mut Runtime,
        resp: TurnOnResponse,
        start_method: StartMethod,
        rom_hash: &Digest,
    ) -> ConnectingState {
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

        ConnectingState::PeeringUp(PeeringState::new(
            start_method,
            Some(socket),
            conf.ggrs.clone(),
            maybe_unlock_url,
        ))
    }
}

#[allow(clippy::large_enum_variant)]
enum NetplayState {
    Disconnected,
    Resuming(Option<ConnectingFlow>, Option<ConnectingFlow>),
    Connecting(Option<ConnectingFlow>),
    Connected(NetplaySession),
}

struct ConnectingFlow {
    start_method: StartMethod,
    state: ConnectingState,
    initial_game_state: LocalGameState,
}
impl ConnectingFlow {
    pub fn new(
        server_config: &NetplayServerConfiguration,
        rt: &mut Runtime,
        rom_hash: &Digest,
        netplay_id: &str,
        start_method: StartMethod,
        initial_game_state: LocalGameState,
    ) -> Self {
        let reqwest_client = reqwest::Client::new();

        let state = match server_config {
            NetplayServerConfiguration::Static(conf) => ConnectingState::new_peering_up(
                rt,
                TurnOnResponse::Full(conf.clone()),
                start_method.clone(),
                rom_hash,
            ),

            NetplayServerConfiguration::TurnOn(server) => {
                let req = reqwest_client.get(format!("{server}/{netplay_id}")).send();
                let (sender, receiver) =
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
                ConnectingState::LoadingNetplayServerConfiguration(receiver)
            }
        };
        Self {
            start_method,
            state,
            initial_game_state,
        }
    }

    fn advance(&mut self, rt: &mut Runtime, rom_hash: &Digest) {
        let state = &mut self.state;
        let start_method = &self.start_method;
        if let Some(new_state) = match state {
            ConnectingState::LoadingNetplayServerConfiguration(conf) => {
                let mut new_state: Option<ConnectingState> = None;
                match conf.try_recv() {
                    Ok(Some(Ok(resp))) => {
                        *state = ConnectingState::new_peering_up(
                            rt,
                            resp,
                            start_method.clone(),
                            rom_hash,
                        );
                    }
                    Ok(None) => (), //No result yet
                    Ok(Some(Err(err))) => {
                        eprintln!("Could not fetch server config :( {:?}", err);
                        //TODO: alert about not being able to fetch server configuration
                        new_state = Some(ConnectingState::Disconnected);
                    }
                    Err(_) => {
                        //Lost the sender, not much to do but go back to disconnected
                        new_state = Some(ConnectingState::Disconnected);
                    }
                }
                new_state
            }
            ConnectingState::PeeringUp(PeeringState {
                start_method,
                socket: maybe_socket,
                ggrs_config,
                unlock_url,
            }) => {
                if let Some(socket) = maybe_socket {
                    socket.update_peers();

                    let connected_peers = socket.connected_peers().count();
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

                        *state = ConnectingState::Synchronizing(SynchonizingState::new(
                            start_method.clone(),
                            Some(
                                sess_build
                                    .start_p2p_session(maybe_socket.take().unwrap())
                                    .unwrap(),
                            ),
                            unlock_url.clone(),
                        ));
                    }
                }
                None
            }
            ConnectingState::Synchronizing(synchronizing_state) => {
                let mut new_state: Option<ConnectingState> = None;
                if let Some(p2p_session) = &mut synchronizing_state.p2p_session {
                    p2p_session.poll_remote_clients();
                    if let SessionState::Running = p2p_session.current_state() {
                        new_state = Some(ConnectingState::Connected(NetplaySession::new(
                            match &synchronizing_state.start_method {
                                StartMethod::Resume(resumable_session) => {
                                    resumable_session.input_mapping.clone()
                                }
                                _ => None,
                            },
                            synchronizing_state.p2p_session.take().unwrap(),
                            match &synchronizing_state.start_method {
                                StartMethod::Resume(resumable_session) => {
                                    let mut game_state = resumable_session.game_state.clone();
                                    game_state.frame = 0;
                                    game_state
                                }
                                _ => self.initial_game_state.clone(),
                            },
                        )));
                    }
                }
                new_state
            }
            ConnectingState::Connected(_) => None,
            //TODO: Try again?
            ConnectingState::Disconnected => None,
        } {
            self.state = new_state;
        }
    }
}

pub struct NetplaySession {
    input_mapping: Option<InputMapping>,
    p2p_session: P2PSession<GGRSConfig>,
    game_state: LocalGameState,
    last_confirmed_game_states: [LocalGameState; 2],
    #[cfg(feature = "debug")]
    pub stats: [stats::NetplayStats; MAX_PLAYERS],
    requested_fps: Fps,
}

impl NetplaySession {
    pub fn new(
        input_mapping: Option<InputMapping>,
        p2p_session: P2PSession<GGRSConfig>,
        game_state: LocalGameState,
    ) -> Self {
        Self {
            input_mapping,
            p2p_session,
            game_state: game_state.clone(),
            last_confirmed_game_states: [game_state.clone(), game_state],
            #[cfg(feature = "debug")]
            stats: [stats::NetplayStats::new(), stats::NetplayStats::new()],
            requested_fps: FPS,
        }
    }

    pub fn advance(
        &mut self,
        inputs: [JoypadInput; MAX_PLAYERS],
        input_mapping: &InputMapping,
    ) -> anyhow::Result<()> {
        let sess = &mut self.p2p_session;
        sess.poll_remote_clients();

        for event in sess.events() {
            if let ggrs::GGRSEvent::Disconnected { addr } = event {
                return Err(anyhow::anyhow!("Lost peer {:?}", addr));
            }
        }

        for handle in sess.local_player_handles() {
            let local_input = 0;
            sess.add_local_input(handle, inputs[input_mapping.map(local_input)].0)?;
        }

        match sess.advance_frame() {
            Ok(requests) => {
                for request in requests {
                    match request {
                        GGRSRequest::LoadGameState { cell, frame } => {
                            println!("Loading (frame {:?})", frame);
                            self.game_state = cell.load().expect("No data found.");
                        }
                        GGRSRequest::SaveGameState { cell, frame } => {
                            assert_eq!(self.game_state.frame, frame);
                            cell.save(frame, Some(self.game_state.clone()), None);
                        }
                        GGRSRequest::AdvanceFrame { inputs } => {
                            let last_saved_frame = self.last_confirmed_game_states[1].frame;
                            if sess.confirmed_frame() >= self.game_state.frame
                                && self.game_state.frame % 10 == 0
                                && self.game_state.frame > last_saved_frame
                            {
                                //We have a confirmed and rendered frame.
                                self.last_confirmed_game_states = [
                                    self.last_confirmed_game_states[1].clone(),
                                    self.game_state.clone(),
                                ];
                            }

                            self.game_state
                                .advance([JoypadInput(inputs[0].0), JoypadInput(inputs[1].0)]);

                            if self.game_state.frame < sess.confirmed_frame() {
                                // Discard the samples for this frame since it's a replay from ggrs. Audio has already been produced and pushed for it.
                                self.game_state.nes.apu.consume_samples();
                            }
                        }
                    }
                }
            }
            Err(ggrs::GGRSError::PredictionThreshold) => {
                //println!(
                //    "Frame {} skipped: PredictionThreshold",
                //    self.game_state.frame
                //);
            }
            Err(ggrs::GGRSError::NotSynchronized) => {}
            Err(e) => eprintln!("Ouch :( {:?}", e),
        }

        #[cfg(feature = "debug")]
        if self.game_state.frame % 30 == 0 {
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
        Ok(())
    }
}

pub struct Netplay {
    rt: Runtime,
    state: NetplayState,
    config: NetplayBuildConfiguration,
    netplay_id: String,
    rom_hash: Digest,
    initial_game_state: LocalGameState,
}

impl Netplay {
    pub fn new(
        config: NetplayBuildConfiguration,
        netplay_id: &mut Option<String>,
        rom_hash: Digest,
        initial_game_state: LocalGameState,
    ) -> Self {
        Self {
            rt: Runtime::new().expect("Could not create an async runtime for Netplay"),
            state: NetplayState::Disconnected,
            config,
            netplay_id: netplay_id
                .get_or_insert_with(|| Uuid::new_v4().to_string())
                .to_string(),
            rom_hash,
            initial_game_state,
        }
    }

    pub fn start(&mut self, start_method: StartMethod) {
        self.state = NetplayState::Connecting(Some(ConnectingFlow::new(
            &self.config.server,
            &mut self.rt,
            &self.rom_hash,
            &self.netplay_id,
            start_method,
            self.initial_game_state.clone(),
        )));
    }

    fn try_attempt(
        attempt: &mut Option<ConnectingFlow>,
        rt: &mut Runtime,
        rom_hash: &Digest,
    ) -> Option<NetplayState> {
        if let Some(connecting_flow) = attempt {
            connecting_flow.advance(rt, rom_hash);
            if let ConnectingFlow {
                state: ConnectingState::Connected(_),
                ..
            } = connecting_flow
            {
                Some(NetplayState::Connecting(attempt.take()))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) {
        if let Some(new_state) = match &mut self.state {
            NetplayState::Disconnected => None,
            NetplayState::Resuming(attempt1, attempt2) => {
                Self::try_attempt(attempt1, &mut self.rt, &self.rom_hash)
                    .or_else(|| Self::try_attempt(attempt2, &mut self.rt, &self.rom_hash))
            }
            NetplayState::Connecting(connecting_flow1) => {
                if let Some(mut connecting_flow) = connecting_flow1.take() {
                    connecting_flow.advance(&mut self.rt, &self.rom_hash);

                    match connecting_flow.state {
                        ConnectingState::Connected(session) => {
                            Some(NetplayState::Connected(session))
                        }
                        ConnectingState::Disconnected => Some(NetplayState::Disconnected),
                        _ => {
                            //No state transition, put it back
                            *connecting_flow1 = Some(connecting_flow);
                            None
                        }
                    }
                } else {
                    None
                }
            }

            NetplayState::Connected(netplay_session) => {
                if let Some(input_mapping) = netplay_session.input_mapping.clone() {
                    if netplay_session.advance(inputs, &input_mapping).is_err() {
                        #[cfg(feature = "debug")]
                        println!(
                            "Could not advance the Netplay session. Resuming to one of the frames ({:?})",
                            netplay_session
                                .last_confirmed_game_states
                                .clone()
                                .map(|s| s.frame)
                        );
                        self.state = NetplayState::Resuming(
                            Some(ConnectingFlow::new(
                                &self.config.server,
                                &mut self.rt,
                                &self.rom_hash,
                                &self.netplay_id,
                                StartMethod::Resume(ResumableNetplaySession::new(
                                    netplay_session.input_mapping.clone(),
                                    netplay_session.last_confirmed_game_states[1].clone(),
                                )),
                                self.initial_game_state.clone(),
                            )),
                            Some(ConnectingFlow::new(
                                &self.config.server,
                                &mut self.rt,
                                &self.rom_hash,
                                &self.netplay_id,
                                StartMethod::Resume(ResumableNetplaySession::new(
                                    netplay_session.input_mapping.clone(),
                                    netplay_session.last_confirmed_game_states[0].clone(),
                                )),
                                self.initial_game_state.clone(),
                            )),
                        );
                    }
                } else {
                    //TODO: Actual input mapping..
                    netplay_session.input_mapping = Some(InputMapping { ids: [0, 1] })
                }
                None
            }
        } {
            self.state = new_state;
        }
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
pub struct BasicConfiguration {
    unlock_url: String,
    conf: StaticNetplayServerConfiguration,
}

#[derive(Deserialize, Debug)]
pub enum TurnOnResponse {
    Basic(BasicConfiguration),
    Full(StaticNetplayServerConfiguration),
}

#[derive(Deserialize, Clone, Debug)]
pub enum NetplayServerConfiguration {
    Static(StaticNetplayServerConfiguration),
    //An external server for fetching TURN credentials
    TurnOn(String),
}
