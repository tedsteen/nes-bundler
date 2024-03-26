use ggrs::{Config, GgrsRequest, P2PSession};
use matchbox_socket::PeerId;

use crate::{
    input::JoypadState,
    nes_state::{FrameData, NesStateHandler, VideoFrame},
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
        video_frame: &mut Option<&mut VideoFrame>,
    ) -> anyhow::Result<Option<FrameData>> {
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

        let mut new_frame = None;
        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("ggrs advance_frame");
            match sess.advance_frame() {
                Ok(requests) => {
                    for request in requests {
                        match request {
                            GgrsRequest::LoadGameState { cell, frame } => {
                                log::debug!("Loading (frame {:?})", frame);
                                self.game_state = cell.load().expect("No data found.");
                            }
                            GgrsRequest::SaveGameState { cell, frame } => {
                                assert_eq!(self.game_state.frame, frame);
                                cell.save(frame, Some(self.game_state.clone()), None);
                            }
                            GgrsRequest::AdvanceFrame { inputs } => {
                                let is_replay = self.game_state.frame <= self.last_handled_frame;
                                let mut n = None;
                                let this_frame_data = self.game_state.advance(
                                    joypad_mapping.map(
                                        [JoypadState(inputs[0].0), JoypadState(inputs[1].0)],
                                        local_player_idx,
                                    ),
                                    if is_replay { &mut n } else { video_frame },
                                );

                                if is_replay {
                                    //This is a replay
                                    // Discard the samples for this frame since it's a replay from ggrs. Audio has already been produced and pushed for it.
                                    self.game_state.discard_samples();
                                } else {
                                    new_frame = this_frame_data;
                                    //This is not a replay
                                    self.last_handled_frame = self.game_state.frame;
                                    if self.game_state.frame % (sess.max_prediction() * 2) as i32
                                        == 0
                                    {
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
        }

        Ok(new_frame.map(|new_frame| {
            if sess.frames_ahead() > 0 {
                FrameData {
                    //Since we are driving emulation using the audio clock, just push some extra audio to slow down.
                    audio: new_frame.audio, //new_frame.audio.repeat(2) //TODO: Figure out why it's not working..
                }
            } else {
                new_frame
            }
        }))
    }
}
