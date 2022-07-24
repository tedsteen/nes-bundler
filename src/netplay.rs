use crate::{audio::Stream, input::JoypadInput, settings::MAX_PLAYERS, Fps, MyGameState, FPS};
use futures::{select, FutureExt};
use futures_timer::Delay;
use ggrs::{Config, GGRSRequest, NetworkStats, P2PSession, SessionBuilder};
use matchbox_socket::{WebRtcSocket, WebRtcSocketConfig, RtcIceServerConfig};
use rusticnes_core::nes::NesState;
use serde::Deserialize;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use tokio::runtime::Runtime;

#[derive(Deserialize)]
pub struct NetplayBuildConfiguration {
    matchbox_server: String,
}

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

pub struct NetplaySession {
    p2p_session: P2PSession<GGRSConfig>,
    frame: Frame,
    pub stats: [NetplayStats; MAX_PLAYERS],
}

#[allow(clippy::large_enum_variant)]
pub enum NetplayState {
    Disconnected,
    Connecting(Option<WebRtcSocket>),
    Connected(NetplaySession),
}

type Frame = i32;
pub struct Netplay {
    rt: Runtime,
    matchbox_server: String,
    pub state: NetplayState,

    pub room_name: String,
    pub max_prediction: usize,
    pub input_delay: usize,
}
impl Netplay {
    pub fn new(netplay_build_config: &NetplayBuildConfiguration) -> Self {
        Netplay {
            rt: Runtime::new().expect("Could not create an async runtime"),
            matchbox_server: netplay_build_config.matchbox_server.clone(),
            state: NetplayState::Disconnected,
            room_name: "example_room".to_string(),
            max_prediction: 12,
            input_delay: 2,
        }
    }

    pub fn connect(&mut self, room: &str) {
        let matchbox_server = &self.matchbox_server;
        //TODO: Enable TURN servers, but can't figure out where to put credentials.
        let (socket, loop_fut) = WebRtcSocket::new_with_config(WebRtcSocketConfig {
            room_url: format!("ws://{matchbox_server}/{room}"),
            ice_server: RtcIceServerConfig {
                urls: vec![
                    "stun:stun.l.google.com:19302".to_string(),
                ],
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

        self.state = NetplayState::Connecting(Some(socket));
    }

    pub fn advance(
        &mut self,
        game_state: &mut MyGameState,
        sound_stream: &mut Stream,
        inputs: [JoypadInput; MAX_PLAYERS],
    ) -> Fps {
        match &mut self.state {
            NetplayState::Disconnected => {
                game_state.advance(inputs);
            }
            NetplayState::Connecting(s) => {
                game_state.advance(inputs);

                if let Some(socket) = s {
                    socket.accept_new_connections();
                    let connected_peers = socket.connected_peers().len();
                    let remaining = MAX_PLAYERS - (connected_peers + 1);
                    if remaining == 0 {
                        let players = socket.players();

                        let mut sess_build = SessionBuilder::<GGRSConfig>::new()
                            .with_num_players(MAX_PLAYERS)
                            .with_max_prediction_window(self.max_prediction)
                            .with_input_delay(self.input_delay)
                            .with_fps(FPS as usize)
                            .expect("invalid fps");

                        for (i, player) in players.into_iter().enumerate() {
                            sess_build = sess_build
                                .add_player(player, i)
                                .expect("failed to add player");
                        }

                        self.state = NetplayState::Connected(NetplaySession {
                            p2p_session: sess_build
                                .start_p2p_session(s.take().unwrap())
                                .expect("failed to start session"),
                            frame: 0,
                            stats: [NetplayStats::new(), NetplayStats::new()],
                        });
                        game_state.nes.reset();
                    }
                }
            }
            NetplayState::Connected(netplay_session) => {
                let sess = &mut netplay_session.p2p_session;
                let frame = &mut netplay_session.frame;
                sess.poll_remote_clients();
                let mut disconnected = false;
                for event in sess.events() {
                    if let ggrs::GGRSEvent::Disconnected { addr } = event {
                        eprintln!("Lost peer {}, disconnecting...", addr);
                        disconnected = true;
                    }
                }
                if disconnected {
                    self.state = NetplayState::Disconnected;
                    return FPS;
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
                                    println!("Loading (frame {:?})", frame);
                                    *game_state = cell.load().expect("No data found.");
                                    *frame = load_state_frame;
                                    sound_stream.drain(); //make sure we don't build up a delay
                                }
                                GGRSRequest::SaveGameState {
                                    cell,
                                    frame: save_state_frame,
                                } => {
                                    assert_eq!(*frame, save_state_frame);
                                    cell.save(*frame, Some(game_state.clone()), None);
                                }
                                GGRSRequest::AdvanceFrame { inputs } => {
                                    //println!("Advancing (frame {:?})", game_runner.get_frame());
                                    game_state.advance([
                                        JoypadInput(inputs[0].0),
                                        JoypadInput(inputs[1].0),
                                    ]);
                                    *frame += 1;
                                }
                            }
                        }
                    }
                    Err(ggrs::GGRSError::PredictionThreshold) => {
                        println!("Frame {} skipped: PredictionThreshold", frame);
                    }
                    Err(ggrs::GGRSError::NotSynchronized) => {
                        //println!("Synchronizing...");
                    }
                    Err(e) => eprintln!("Ouch :( {:?}", e),
                }

                if *frame % 30 == 0 {
                    for i in 0..MAX_PLAYERS {
                        if let Ok(stats) = sess.network_stats(i as usize) {
                            if !sess.local_player_handles().contains(&i) {
                                netplay_session.stats[i].push_stats(stats);
                            }
                        }
                    }
                }
                if sess.frames_ahead() > 0 {
                    return (FPS as f32 * 0.9) as u32;
                }
            }
        }
        FPS
    }
}
