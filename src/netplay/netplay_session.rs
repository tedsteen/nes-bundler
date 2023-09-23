use ggrs::{Config, GGRSRequest, P2PSession};
use matchbox_socket::PeerId;

use crate::{input::JoypadInput, nes_state::NesStateHandler, settings::MAX_PLAYERS, Fps, FPS};

use super::{connecting_state::StartMethod, JoypadMapping, NetplayNesState};

#[derive(Debug)]
pub struct GGRSConfig;
impl Config for GGRSConfig {
    type Input = u8;
    type State = NetplayNesState;
    type Address = PeerId;
}

pub struct NetplaySession {
    pub p2p_session: P2PSession<GGRSConfig>,
    pub game_state: NetplayNesState,
    pub last_handled_frame: i32,
    pub last_confirmed_game_states: [NetplayNesState; 2],
    #[cfg(feature = "debug")]
    pub stats: [super::stats::NetplayStats; MAX_PLAYERS],
    pub requested_fps: Fps,
}

impl NetplaySession {
    pub fn new(start_method: StartMethod, p2p_session: P2PSession<GGRSConfig>) -> Self {
        let mut initial_game_state = match &start_method {
            StartMethod::Join(start_state, _)
            | StartMethod::Resume(start_state)
            | StartMethod::MatchWithRandom(start_state) => start_state.clone().game_state,
        };
        //Start counting from 0 to be in sync with ggrs frame counter.
        initial_game_state.frame = 0;

        Self {
            p2p_session,
            game_state: initial_game_state.clone(),
            last_confirmed_game_states: [initial_game_state.clone(), initial_game_state],
            last_handled_frame: -1,
            #[cfg(feature = "debug")]
            stats: [
                super::stats::NetplayStats::new(),
                super::stats::NetplayStats::new(),
            ],
            requested_fps: FPS,
        }
    }

    pub fn get_local_player_idx(&self) -> usize {
        //There should be only one.
        *self
            .p2p_session
            .local_player_handles()
            .first()
            .unwrap_or(&0)
    }

    pub fn advance(
        &mut self,
        inputs: [JoypadInput; MAX_PLAYERS],
        joypad_mapping: &JoypadMapping,
    ) -> anyhow::Result<()> {
        let local_player_idx = self.get_local_player_idx();
        let sess = &mut self.p2p_session;
        sess.poll_remote_clients();

        for event in sess.events() {
            if let ggrs::GGRSEvent::Disconnected { addr } = event {
                return Err(anyhow::anyhow!("Lost peer {:?}", addr));
            }
        }

        for handle in sess.local_player_handles() {
            sess.add_local_input(handle, *inputs[0])?;
        }
        match sess.advance_frame() {
            Ok(requests) => {
                for request in requests {
                    match request {
                        GGRSRequest::LoadGameState { cell, frame } => {
                            log::debug!("Loading (frame {:?})", frame);
                            self.game_state = cell.load().expect("No data found.");
                        }
                        GGRSRequest::SaveGameState { cell, frame } => {
                            assert_eq!(self.game_state.frame, frame);
                            cell.save(frame, Some(self.game_state.clone()), None);
                        }
                        GGRSRequest::AdvanceFrame { inputs } => {
                            self.game_state.advance(joypad_mapping.map(
                                [JoypadInput(inputs[0].0), JoypadInput(inputs[1].0)],
                                local_player_idx,
                            ));
                            self.game_state.frame += 1;

                            if self.last_handled_frame >= self.game_state.frame {
                                //This is a replay
                                // Discard the samples for this frame since it's a replay from ggrs. Audio has already been produced and pushed for it.
                                self.game_state.apu.consume_samples();
                            } else {
                                //This is not a replay
                                self.last_handled_frame = self.game_state.frame;
                                if self.game_state.frame % (sess.max_prediction() * 2) as i32 == 0 {
                                    self.last_confirmed_game_states = [
                                        self.last_confirmed_game_states[1].clone(),
                                        self.game_state.clone(),
                                    ];
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                log::warn!("Frame {} skipped: {:?}", self.game_state.frame, e)
            }
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
