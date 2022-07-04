use futures::{select, FutureExt};
use futures_timer::Delay;
use ggrs::{Config, SessionBuilder, P2PSession, GGRSRequest};
use matchbox_socket::WebRtcSocket;
use std::time::Duration;

use crate::{input::{StaticJoypadInput, JoypadInput}, GameRunner, GameRunnerState, MyGameState};

#[derive(Debug)]
pub(crate) struct GGRSConfig;
impl Config for GGRSConfig {
    type Input = u8;
    type State = MyGameState;
    type Address = String;
}

#[allow(clippy::large_enum_variant)]
enum NetplayState {
    Connecting(Option<WebRtcSocket>),
    Connected(P2PSession<GGRSConfig>)
}
pub struct Netplay {
    fps: u32,
    players: usize,
    state: NetplayState
}
impl Netplay {
    pub(crate) fn advance(&mut self, inputs: Vec<StaticJoypadInput>, game_runner: &mut GameRunner) {
        match &mut self.state {
            NetplayState::Connecting(s) => {
                if let Some(socket) = s {
                    socket.accept_new_connections();
                    let connected_peers = socket.connected_peers().len();
                    let remaining = self.players - (connected_peers + 1);
                    if remaining == 0 {
                        //let socket = socket.take().unwrap();

                        // extract final player list
                        let players = socket.players();

                        let max_prediction = 12;

                        // create a GGRS P2P session
                        let mut sess_build = SessionBuilder::<GGRSConfig>::new()
                            .with_num_players(self.players)
                            .with_max_prediction_window(max_prediction)
                            .with_input_delay(2)
                            .with_fps(self.fps as usize)
                            .expect("invalid fps");

                        for (i, player) in players.into_iter().enumerate() {
                            sess_build = sess_build
                                .add_player(player, i)
                                .expect("failed to add player");
                        }

                        // start the GGRS session
                        self.state = NetplayState::Connected(sess_build
                            .start_p2p_session(s.take().unwrap())
                            .expect("failed to start session"));
                    }    
                }
                
            },
            NetplayState::Connected(sess) => {
                sess.poll_remote_clients();
                for event in sess.events() {
                    println!("Event: {:?}", event);
                }
                game_runner.run_slow = sess.frames_ahead() > 0;

                for handle in sess.local_player_handles() {
                    sess.add_local_input(handle, inputs[handle].to_u8()).unwrap();
                }

                match sess.advance_frame() {
                    Ok(requests) => {
                        for request in requests {
                            match request {
                                GGRSRequest::LoadGameState { cell, .. } => {
                                    if let GameRunnerState::Playing(state) = &mut game_runner.state {
                                        println!("Loading (frame {:?})", state.frame);
                                        let loaded_state = cell.load().expect("No data found.");
                                        state.nes = loaded_state.nes;
                                        state.frame = loaded_state.frame;
                                        state.nes.apu.consume_samples(); //Clear audio buffer so we don't build up a delay
                                    }
                                },
                                GGRSRequest::SaveGameState { cell, frame } => {
                                    if let GameRunnerState::Playing(state) = &mut game_runner.state {
                                        assert_eq!(state.frame, frame);
                                        if state.frame - frame != 0 {
                                            eprintln!("{:?} should be 0", state.frame - frame);
                                        }
                                        cell.save(frame, Some(state.clone()), None);
                                    }
                                },
                                GGRSRequest::AdvanceFrame { inputs } => {
                                    //println!("Advancing (frame {:?})", game_runner.get_frame());
                                    game_runner.advance(vec![StaticJoypadInput(inputs[0].0), StaticJoypadInput(inputs[1].0)])
                                }
                            }
                        }
                    }
                    Err(ggrs::GGRSError::PredictionThreshold) => {
                        if let GameRunnerState::Playing(state) = &mut game_runner.state {
                            println!(
                                "Frame {} skipped: PredictionThreshold", state.frame
                            );
                        }
                    }
                    Err(ggrs::GGRSError::NotSynchronized) => {
                        println!("Synchronizing...");
                    }
                    Err(e) => eprintln!("Ouch :( {:?}", e),
                }

                //regularily print networks stats
                if game_runner.get_frame() % 120 == 0 {
                    for i in 0..self.players {
                        if let Ok(stats) = sess.network_stats(i as usize) {
                            println!("NetworkStats to player {}: {:?}", i, stats);
                        }
                    }
                }
            }
        }
    }
}
pub(crate) async fn connect(fps: u32, players: usize) -> Netplay {
    println!("Connecting...");

    let (socket, loop_fut) = WebRtcSocket::new("ws://matchbox.marati.s3n.io:3536/example_room");

    println!("my id is {:?}", socket.id());

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

    Netplay { state: NetplayState::Connecting(Some(socket)), fps, players }
}