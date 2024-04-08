use ggrs::{Config, GgrsRequest, P2PSession};
use matchbox_socket::PeerId;

use crate::{
    emulation::{Emulator, NESBuffers, NesStateHandler},
    input::JoypadState,
    settings::MAX_PLAYERS,
};

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
}

impl NetplaySession {
    pub fn new(start_method: StartMethod, p2p_session: P2PSession<GGRSConfig>) -> Self {
        let mut game_state = match &start_method {
            StartMethod::Join(start_state, _)
            | StartMethod::Resume(start_state)
            | StartMethod::MatchWithRandom(start_state) => start_state.clone().game_state,
        };
        //Start counting from 0 to be in sync with ggrs frame counter.
        game_state.frame = 0;

        Self {
            p2p_session,
            game_state: game_state.clone(),
            last_confirmed_game_states: [game_state.clone(), game_state],
            last_handled_frame: -1,
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
        joypad_state: [JoypadState; MAX_PLAYERS],
        joypad_mapping: &JoypadMapping,
        buffers: &mut NESBuffers,
    ) -> anyhow::Result<()> {
        #[cfg(feature = "debug")]
        puffin::profile_function!();

        let local_player_idx = self.get_local_player_idx();
        let sess = &mut self.p2p_session;

        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("ggrs advance_frame");
            sess.poll_remote_clients();
        }

        for event in sess.events() {
            if let ggrs::GgrsEvent::Disconnected { addr } = event {
                return Err(anyhow::anyhow!("Lost peer {:?}", addr));
            }
        }

        for handle in sess.local_player_handles() {
            sess.add_local_input(handle, *joypad_state[0])?;
        }

        #[cfg(feature = "debug")]
        puffin::profile_scope!("ggrs advance_frame");
        match sess.advance_frame() {
            Ok(requests) => {
                for request in requests {
                    match request {
                        GgrsRequest::LoadGameState { cell, frame } => {
                            log::debug!("Loading (frame {:?})", frame);
                            self.game_state = cell.load().expect("ggrs state to load");
                        }
                        GgrsRequest::SaveGameState { cell, frame } => {
                            assert_eq!(self.game_state.frame, frame);
                            cell.save(frame, Some(self.game_state.clone()), None);
                        }
                        GgrsRequest::AdvanceFrame { inputs } => {
                            let is_replay = self.game_state.frame <= self.last_handled_frame;
                            let no_buffers = &mut NESBuffers {
                                audio: None,
                                video: None,
                            };
                            self.game_state.advance(
                                joypad_mapping.map(
                                    [JoypadState(inputs[0].0), JoypadState(inputs[1].0)],
                                    local_player_idx,
                                ),
                                if is_replay { no_buffers } else { buffers },
                            );

                            if !is_replay {
                                //This is not a replay
                                self.last_handled_frame = self.game_state.frame;
                                if self.game_state.frame % (sess.max_prediction() * 2) as i32 == 0 {
                                    self.last_confirmed_game_states = [
                                        self.last_confirmed_game_states[1].clone(),
                                        self.game_state.clone(),
                                    ];
                                }
                            }

                            self.game_state.frame += 1;
                        }
                    }
                }
            }
            Err(e) => {
                log::warn!("Frame {} skipped: {:?}", self.game_state.frame, e)
            }
        }

        *Emulator::emulation_speed_mut() = if sess.frames_ahead() > 0 {
            log::debug!(
                "Frames ahead: {:?}, slowing down emulation",
                sess.frames_ahead()
            );
            //https://www.desmos.com/calculator/zbntsowijd
            0.8_f32.max(1.0 - 0.1 * (0.2 * sess.frames_ahead() as f32).powf(2.0))
        } else {
            1.0
        };
        Ok(())
    }
}
