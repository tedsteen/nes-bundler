use futures::{select, FutureExt};
use futures_timer::Delay;
use ggrs::{Config, P2PSession, GGRSRequest, SessionBuilder};
use matchbox_socket::WebRtcSocket;
use std::time::Duration;

use crate::{MyGameState, GameRunner, input::{StaticJoypadInput, JoypadInput}, settings::MAX_PLAYERS};

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
    Connected(P2PSession<GGRSConfig>)
}

pub(crate) fn connect(room: &str) -> WebRtcSocket {
    let (socket, loop_fut) = WebRtcSocket::new(format!("ws://matchbox.marati.s3n.io:3536/{}", room));

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

    socket
}

pub(crate) fn advance(game_runner: &mut GameRunner, inputs: Vec<StaticJoypadInput>) {
    let settings = &mut game_runner.settings;
    match &mut settings.netplay_state {
        NetplayState::Disconnected => {
            game_runner.state.advance(inputs, &mut game_runner.sound_stream);
            game_runner.state.frame = 0;
        },
        NetplayState::Connecting(s) => {
            game_runner.state.advance(inputs, &mut game_runner.sound_stream);
            game_runner.state.frame = 0;
            
            if let Some(socket) = s {
                socket.accept_new_connections();
                let connected_peers = socket.connected_peers().len();
                let remaining = MAX_PLAYERS - (connected_peers + 1);
                if remaining == 0 {
                    let players = socket.players();

                    let max_prediction = 12;
                    
                    let mut sess_build = SessionBuilder::<GGRSConfig>::new()
                        .with_num_players(MAX_PLAYERS)
                        .with_max_prediction_window(max_prediction)
                        .with_input_delay(2)
                        .with_fps(settings.fps as usize)
                        .expect("invalid fps");

                    for (i, player) in players.into_iter().enumerate() {
                        sess_build = sess_build
                            .add_player(player, i)
                            .expect("failed to add player");
                    }

                    settings.netplay_state = NetplayState::Connected(sess_build
                        .start_p2p_session(s.take().unwrap())
                        .expect("failed to start session"));
                }
            }
        },
        NetplayState::Connected(sess) => {
            if game_runner.state.frame == 0 {
                game_runner.state.nes.reset();
            }

            sess.poll_remote_clients();
            for event in sess.events() {
                println!("Event: {:?}", event);
            }
            game_runner.run_slow = sess.frames_ahead() > 0;

            for handle in sess.local_player_handles() {
                let local_input = 0;
                sess.add_local_input(handle, inputs[local_input].to_u8()).unwrap();
            }

            match sess.advance_frame() {
                Ok(requests) => {
                    for request in requests {
                        match request {
                            GGRSRequest::LoadGameState { cell, .. } => {
                                let game_state = &mut game_runner.state;
                                println!("Loading (frame {:?})", game_state.frame);
                                let loaded_state = cell.load().expect("No data found.");
                                game_state.nes = loaded_state.nes;
                                game_state.frame = loaded_state.frame;
                                game_state.nes.apu.consume_samples(); //Clear audio buffer so we don't build up a delay
                            },
                            GGRSRequest::SaveGameState { cell, frame } => {
                                let game_state = &mut game_runner.state;
                                assert_eq!(game_state.frame, frame);
                                cell.save(frame, Some(game_state.clone()), None);
                            },
                            GGRSRequest::AdvanceFrame { inputs } => {
                                //println!("Advancing (frame {:?})", game_runner.get_frame());
                                game_runner.state.advance(vec![StaticJoypadInput(inputs[0].0), StaticJoypadInput(inputs[1].0)], &mut game_runner.sound_stream)
                            }
                        }
                    }
                }
                Err(ggrs::GGRSError::PredictionThreshold) => {
                    let game_state = &mut game_runner.state;
                    println!(
                        "Frame {} skipped: PredictionThreshold", game_state.frame
                    );
                }
                Err(ggrs::GGRSError::NotSynchronized) => {
                    //println!("Synchronizing...");
                }
                Err(e) => eprintln!("Ouch :( {:?}", e),
            }

            //regularily print networks stats
            if game_runner.state.frame % 120 == 0 {
                for i in 0..MAX_PLAYERS {
                    if let Ok(stats) = sess.network_stats(i as usize) {
                        println!("NetworkStats to player {}: {:?}", i, stats);
                    }
                }
            }
        }
    }
}