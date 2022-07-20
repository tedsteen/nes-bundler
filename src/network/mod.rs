use crate::{input::JoypadInput, settings::MAX_PLAYERS, Fps, MyGameState, FPS};
use futures::{select, FutureExt};
use futures_timer::Delay;
use ggrs::{Config, GGRSRequest, P2PSession, SessionBuilder};
use matchbox_socket::WebRtcSocket;
use rusticnes_core::nes::NesState;
use std::time::Duration;

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
pub(crate) struct GGRSConfig;
impl Config for GGRSConfig {
    type Input = u8;
    type State = MyGameState;
    type Address = String;
}

#[allow(clippy::large_enum_variant)]
pub(crate) enum NetplayState {
    Disconnected,
    Connecting(Option<WebRtcSocket>),
    Connected((P2PSession<GGRSConfig>, Frame)),
}

type Frame = i32;
pub(crate) struct Netplay {
    pub(crate) state: NetplayState,

    pub(crate) room_name: String,
    pub(crate) max_prediction: usize,
    pub(crate) input_delay: usize,
}
impl Netplay {
    pub(crate) fn new() -> Self {
        Netplay {
            state: NetplayState::Disconnected,
            room_name: "example_room".to_string(),
            max_prediction: 12,
            input_delay: 2,
        }
    }

    pub(crate) fn connect(&mut self, room: &str) {
        let (socket, loop_fut) =
            WebRtcSocket::new(format!("ws://matchbox.marati.s3n.io:3536/{}", room));

        let loop_fut = loop_fut.fuse();
        tokio::spawn(async move {
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

    pub(crate) fn advance(
        &mut self,
        game_state: &mut MyGameState,
        inputs: [&JoypadInput; MAX_PLAYERS],
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

                        self.state = NetplayState::Connected((
                            sess_build
                                .start_p2p_session(s.take().unwrap())
                                .expect("failed to start session"),
                            0,
                        ));
                        game_state.nes.reset();
                    }
                }
            }
            NetplayState::Connected((sess, frame)) => {
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
                                        &JoypadInput(inputs[0].0),
                                        &JoypadInput(inputs[1].0),
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

                //regularily print networks stats
                if *frame % 120 == 0 {
                    for i in 0..MAX_PLAYERS {
                        if let Ok(stats) = sess.network_stats(i as usize) {
                            println!("NetworkStats to player {}: {:?}", i, stats);
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
