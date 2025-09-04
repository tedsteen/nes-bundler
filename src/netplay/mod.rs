use self::connection::StartMethod;
use std::time::Instant;

use anyhow::Result;
use tokio::sync::watch::Sender;

use crate::{
    emulation::{
        LocalNesState, NESBuffers, NesStateHandler, SharedNetplayConnectedState, SharedNetplayState,
    },
    input::JoypadState,
    netplay::{
        connection::{ConnectingSession, JoinOrHost, NetplayConnection},
        session::{AdvanceError, NetplaySession},
    },
    settings::MAX_PLAYERS,
};

pub mod configuration;
pub mod connection;
pub mod gui;
pub mod session;

#[cfg(feature = "debug")]
mod stats;

pub const MAX_ROOM_NAME_LEN: u8 = 4;

pub struct Netplay {
    shared_state_sender: Sender<SharedNetplayState>,
    session: Option<NetplaySession>,
    local_play_nes_state: LocalNesState,
}

impl Netplay {
    pub fn new(
        local_play_nes_state: LocalNesState,
        shared_state_sender: Sender<SharedNetplayState>,
    ) -> Self {
        Self {
            shared_state_sender: shared_state_sender,
            local_play_nes_state,
            session: None,
        }
    }

    fn swap_session(&mut self, netplay_connection: NetplayConnection) {
        let _ = self.shared_state_sender.send(SharedNetplayState::Connected(
            SharedNetplayConnectedState::Synchronizing,
        ));
        self.session
            .replace(NetplaySession::new(netplay_connection));
    }

    async fn start(&mut self, start_method: StartMethod) -> Result<()> {
        let connecting_session = ConnectingSession::connect(start_method.clone());

        let _ = self
            .shared_state_sender
            .send(SharedNetplayState::Connecting(connecting_session.state));

        self.swap_session(connecting_session.netplay_connection.await?);
        Ok(())
    }

    async fn do_resume(&mut self) {
        //TODO: Popup/info about the error? Or perhaps put the reason for the resume in the resume state below?
        //TODO: PeerLost is peraps only one of the failures?
        if let Some(current_session) = &mut self.session {
            log::debug!(
                "Resuming netplay to one of the frames {:?} and {:?}",
                current_session.last_confirmed_game_state1.ggrs_frame,
                current_session.last_confirmed_game_state2.ggrs_frame
            );
            let _ = self.shared_state_sender.send(SharedNetplayState::Resuming);

            let netplay_server_configuration = &current_session.netplay_server_configuration;

            let attempt1 = ConnectingSession::connect(StartMethod::Resume(
                netplay_server_configuration.clone(),
                current_session.last_confirmed_game_state1.clone(),
            ))
            .netplay_connection;

            let attempt2 = ConnectingSession::connect(StartMethod::Resume(
                netplay_server_configuration.clone(),
                current_session.last_confirmed_game_state2.clone(),
            ))
            .netplay_connection;

            futures::pin_mut!(attempt1);
            futures::pin_mut!(attempt2);

            let new_connection = loop {
                tokio::select! {
                    Ok(c) = attempt1 => {
                        break c;
                    }
                    Ok(c) = attempt2 => {
                        break c;
                    }
                }
            };
            self.swap_session(new_connection);
        }
    }

    pub(crate) async fn find_game(&mut self) {
        if let Err(e) = self.start(StartMethod::MatchWithRandom).await {
            panic!("TODO: Failed to match with random game: {e:?}");
        }
    }

    pub(crate) async fn host_game(&mut self) {
        use rand::distr::{Alphanumeric, SampleString};
        let room_name = Alphanumeric
            .sample_string(&mut rand::rng(), MAX_ROOM_NAME_LEN.into())
            .to_uppercase();
        if let Err(e) = self
            .start(StartMethod::Start(room_name, JoinOrHost::Host))
            .await
        {
            panic!("TODO: Failed to host game: {e:?}");
        }
    }

    pub(crate) async fn join_game(&mut self, room_name: &str) {
        if let Err(e) = self
            .start(StartMethod::Start(room_name.to_string(), JoinOrHost::Join))
            .await
        {
            panic!("TODO: Failed to join game: {e:?}");
        }
    }

    pub(crate) fn cancel_connect(&self) {
        todo!()
    }

    pub(crate) fn retry_connect(&self) {
        todo!()
    }

    pub(crate) fn disconnect(&self) {
        todo!()
    }

    pub(crate) fn resume(&self) {
        todo!()
    }
}

impl NesStateHandler for Netplay {
    async fn advance(
        &mut self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        buffers: &mut NESBuffers<'_>,
    ) {
        if let Some(session) = &mut self.session {
            let ggrs_is_running = matches!(
                session.p2p_session.current_state(),
                ggrs::SessionState::Running
            );
            let we_are_running = matches!(
                *self.shared_state_sender.borrow(),
                SharedNetplayState::Connected(SharedNetplayConnectedState::Running(..), ..)
            );

            let new_state = if we_are_running && !ggrs_is_running {
                Some(SharedNetplayConnectedState::Synchronizing)
            } else if !we_are_running && ggrs_is_running {
                Some(SharedNetplayConnectedState::Running(Instant::now()))
            } else {
                None
            };

            if let Some(new_state) = new_state {
                let _ = self
                    .shared_state_sender
                    .send(SharedNetplayState::Connected(new_state));
            }

            if let Err(AdvanceError::LostPeer) = session.advance(joypad_state, buffers).await {
                self.do_resume().await;
            }
        } else {
            self.local_play_nes_state
                .advance(joypad_state, buffers)
                .await;
        }
    }

    fn reset(&mut self, hard: bool) {
        // Only possible to reset the nes on a local game
        self.local_play_nes_state.reset(hard);
    }

    fn set_speed(&mut self, speed: f32) {
        if let Some(session) = &mut self.session {
            session.current_game_state.nes_state.set_speed(speed);
        } else {
            self.local_play_nes_state.set_speed(speed)
        }
    }

    fn get_samples_per_frame(&self) -> f32 {
        if let Some(session) = &self.session {
            session.current_game_state.nes_state.get_samples_per_frame()
        } else {
            self.local_play_nes_state.get_samples_per_frame()
        }
    }

    fn save_sram(&self) -> Option<&[u8]> {
        if self.session.is_none() {
            self.local_play_nes_state.save_sram()
        } else {
            //Only possible to save when playing locally
            None
        }
    }

    fn frame(&self) -> u32 {
        if let Some(session) = &self.session {
            session.current_game_state.nes_state.frame()
        } else {
            self.local_play_nes_state.frame()
        }
    }
}

// impl NesStateHandler for NetplayStateHandler {
//     fn advance(&mut self, joypad_state: [JoypadState; MAX_PLAYERS], buffers: &mut NESBuffers) {
//         #[cfg(feature = "debug")]
//         if let Some(NetplayState::Connected(netplay)) = &mut self.netplay {
//             let sess = &netplay.netplay_session.p2p_session;
//             if netplay.netplay_session.current_game_state.frame % 30 == 0 {
//                 puffin::profile_scope!("Netplay stats");
//                 for i in 0..MAX_PLAYERS {
//                     if let Ok(stats) = sess.network_stats(i) {
//                         if !sess.local_player_handles().contains(&i) {
//                             netplay.stats[i].push_stats(stats);
//                         }
//                     }
//                 }
//             };
//         }
//         self.advance(joypad_state, buffers);
//     }
// }
