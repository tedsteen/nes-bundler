use ggrs::{Config, GGRSRequest, P2PSession};
use matchbox_socket::PeerId;

use crate::{input::JoypadInput, settings::MAX_PLAYERS, Fps, LocalGameState, FPS};

use super::InputMapping;

#[derive(Debug)]
pub struct GGRSConfig;
impl Config for GGRSConfig {
    type Input = u8;
    type State = LocalGameState;
    type Address = PeerId;
}

pub struct NetplaySession {
    pub input_mapping: Option<InputMapping>,
    pub p2p_session: P2PSession<GGRSConfig>,
    pub game_state: LocalGameState,
    pub last_handled_frame: i32,
    pub last_confirmed_game_states: [LocalGameState; 2],
    #[cfg(feature = "debug")]
    pub stats: [super::stats::NetplayStats; MAX_PLAYERS],
    pub requested_fps: Fps,
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
            last_confirmed_game_states: [game_state.clone(), game_state.clone()],
            last_handled_frame: -1,
            #[cfg(feature = "debug")]
            stats: [
                super::stats::NetplayStats::new(),
                super::stats::NetplayStats::new(),
            ],
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
                            self.game_state
                                .advance([JoypadInput(inputs[0].0), JoypadInput(inputs[1].0)]);

                            if self.last_handled_frame >= self.game_state.frame {
                                // Discard the samples for this frame since it's a replay from ggrs. Audio has already been produced and pushed for it.
                                self.game_state.nes.apu.consume_samples();
                            } else {
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
