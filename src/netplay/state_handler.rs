use std::{cell::RefCell, rc::Rc};

use ggrs::{SessionBuilder, SessionState};

use crate::{
    input::JoypadInput,
    settings::{Settings, MAX_PLAYERS},
    Bundle, Fps, LocalGameState, StateHandler, FPS,
};

use super::{
    ConnectedState, ConnectingState, GGRSConfig, InputMapping, Netplay, NetplaySession,
    NetplaySessionState, NetplayState, PeeringState, SynchonizingState,
};

pub struct NetplayStateHandler {
    pub netplay: Rc<RefCell<Netplay>>,
    game_state: LocalGameState,
    initial_game_state: LocalGameState,
}

impl StateHandler for NetplayStateHandler {
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Fps {
        let netplay = &mut self.netplay.borrow_mut();
        if let Some(new_state) = match &mut netplay.state {
            NetplayState::Disconnected => {
                self.game_state.advance(inputs);
                None
            }
            NetplayState::Connecting(start_method, connecting_state) => {
                match connecting_state {
                    ConnectingState::LoadingNetplayServerConfiguration(conf) => {
                        self.game_state.advance(inputs);
                        match conf.try_recv() {
                            Ok(Some(Ok(resp))) => {
                                *connecting_state = self
                                    .netplay
                                    .borrow_mut()
                                    .start_peering(resp, start_method.clone());
                            }
                            Ok(None) => (), //No result yet
                            Ok(Some(Err(err))) => {
                                eprintln!("Could not fetch server config :( {:?}", err);
                                //TODO: alert about not being able to fetch server configuration
                                netplay.state = NetplayState::Disconnected;
                            }
                            Err(_) => {
                                //Lost the sender, not much to do but go back to disconnected
                                netplay.state = NetplayState::Disconnected;
                            }
                        }

                        None
                    }
                    ConnectingState::PeeringUp(PeeringState {
                        socket: maybe_socket,
                        ggrs_config,
                        unlock_url,
                    }) => {
                        self.game_state.advance(inputs);
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
                                *connecting_state =
                                    ConnectingState::Synchronizing(SynchonizingState::new(
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
                        let mut new_state = None;
                        if let Some(p2p_session) = &mut synchronizing_state.p2p_session {
                            p2p_session.poll_remote_clients();
                            if let SessionState::Running = p2p_session.current_state() {
                                self.game_state = self.initial_game_state.clone();

                                new_state = Some(NetplayState::Connected(
                                    NetplaySession::new(
                                        synchronizing_state.p2p_session.take().unwrap(),
                                    ),
                                    ConnectedState::MappingInput,
                                ));
                            }
                        }
                        new_state
                    }
                }
            }

            NetplayState::Connected(netplay_session, connected_state) => {
                match connected_state {
                    ConnectedState::MappingInput => {
                        netplay_session.advance(&mut self.game_state, inputs);
                        //TODO: Actual input mapping..
                        *connected_state = ConnectedState::Playing(InputMapping { ids: [0, 1] });
                    }
                    ConnectedState::Playing(_input_mapping) => {
                        netplay_session.advance(&mut self.game_state, inputs);
                    }
                }

                if let NetplaySessionState::DisconnectedPeers = netplay_session.state {
                    // For now, just disconnect if we loose peers
                    netplay.state = NetplayState::Disconnected;
                }
                None
            }
        } {
            netplay.state = new_state;
        }

        if let NetplayState::Connected(netplay_session, _) = &netplay.state {
            netplay_session.requested_fps
        } else {
            FPS
        }
    }

    fn consume_samples(&mut self) -> Vec<i16> {
        self.game_state.consume_samples()
    }
    fn get_frame(&self) -> &Vec<u16> {
        self.game_state.get_frame()
    }
    fn save(&self) -> Vec<u8> {
        self.game_state.save()
    }
    fn load(&mut self, data: &mut Vec<u8>) {
        self.game_state.load(data)
    }
}

impl NetplayStateHandler {
    pub fn new(game_state: LocalGameState, bundle: &Bundle, settings: &mut Settings) -> Self {
        let netplay = std::rc::Rc::new(std::cell::RefCell::new(Netplay::new(
            &bundle.config.netplay,
            settings,
            md5::compute(&bundle.rom),
        )));

        NetplayStateHandler {
            netplay,
            game_state: game_state.clone(),
            initial_game_state: game_state,
        }
    }
}
