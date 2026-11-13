use std::{
    fmt::Debug,
    mem,
    time::{Duration, Instant},
};

use anyhow::{Context, Error};
use ggrs::PlayerType;
use ggrs::{Config, GgrsRequest, P2PSession, SessionBuilder};
use matchbox_socket::{PeerId, WebRtcSocket};
use tokio::select;

use crate::{
    bundle::Bundle,
    emulation::{LocalNesState, NESBuffers, NesStateHandler},
    input::JoypadState,
    netplay::{
        ConnectionPath, SharedNetplayConnectedState, SharedNetplayState,
        configuration::StaticNetplayServerConfiguration,
        connection::{ConnectingSession, NetplayConnection, PeerUnreachable, StartMethod},
    },
    recurrence::Recurrence,
    settings::MAX_PLAYERS,
};
#[derive(Clone, Debug)]
pub enum JoypadMapping {
    P1,
    P2,
}

impl JoypadMapping {
    fn map(
        &self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        local_player_idx: usize,
    ) -> [JoypadState; MAX_PLAYERS] {
        match self {
            JoypadMapping::P1 => {
                if local_player_idx == 0 {
                    [joypad_state[0], joypad_state[1]]
                } else {
                    [joypad_state[1], joypad_state[0]]
                }
            }
            JoypadMapping::P2 => {
                if local_player_idx == 0 {
                    [joypad_state[1], joypad_state[0]]
                } else {
                    [joypad_state[0], joypad_state[1]]
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct NetplayNesState {
    pub nes_state: LocalNesState,
    pub ggrs_frame: i32,
    joypad_mapping: JoypadMapping,
}
impl NetplayNesState {
    /// The bundle's netplay ROM, in its default region: both players have to run the same game.
    pub fn new(joypad_mapping: JoypadMapping) -> anyhow::Result<Self> {
        let bundle = Bundle::current();
        Ok(Self {
            nes_state: LocalNesState::start_rom(
                &bundle.netplay_rom,
                false,
                bundle.config.default_region(),
            )
            .context("Could not start the netplay ROM")?,
            ggrs_frame: 0,
            joypad_mapping,
        })
    }
}

#[derive(Debug)]
pub struct GGRSConfig;
impl Config for GGRSConfig {
    type Input = u8;
    // Same as earlier ggrs versions: assume the remote keeps holding what it held last.
    type InputPredictor = ggrs::PredictRepeatLast;
    type State = NetplayNesState;
    type Address = PeerId;
}

pub struct ConnectedNetplaySession {
    local_player_index: usize,
    pub p2p_session: P2PSession<GGRSConfig>,
    pub netplay_server_configuration: StaticNetplayServerConfiguration,

    pub last_handled_ggrs_frame: i32,
    pub current_game_state: NetplayNesState,
    pub older_confirmed_state: NetplayNesState,
    pub newer_confirmed_state: NetplayNesState,
    start_time: Instant,
    last_running_time: Instant,
    /// How much slower (ahead of the remote) or faster (behind it) than the display to run, so the
    /// two meet. Slows the emulator's frame clock (see `NetplaySession::pace`) and stretches the
    /// audio to match.
    catch_up_speed: f32,
    /// Seconds per frame, for easing `catch_up_speed`.
    frame_time: f32,
    /// Last `frames_ahead` logged, to log only when it changes.
    logged_frames_ahead: i32,
    /// Frames in a row GGRS has had us wait for the other player's input.
    waited_frames: u32,
    /// The game freezing for the other player's input, or rolling back far, to tell a laggy
    /// connection from a one-off hiccup; and when it last did while laggy.
    lag: Recurrence,
    last_lag: Option<Instant>,
    log: NetplayLog,
    /// Kept for querying the WebRTC connection; GGRS owns its data channel.
    socket: WebRtcSocket,
    remote_peer: Option<PeerId>,
    connection_path: Option<ConnectionPath>,
    _loop_task: crate::netplay::connection::AbortOnDrop,
}

impl ConnectedNetplaySession {
    fn new(netplay_connection: NetplayConnection) -> anyhow::Result<Self> {
        let mut socket = netplay_connection.socket;
        let netplay_server_configuration = netplay_connection.netplay_server_configuration.clone();
        let initial_state = netplay_connection.initial_state;
        let loop_task = netplay_connection.loop_task;

        let fps = Bundle::current().config.default_region().to_fps();
        let ggrs_config = &netplay_server_configuration.ggrs;
        let mut sess_build = SessionBuilder::<GGRSConfig>::new()
            .with_num_players(MAX_PLAYERS)?
            .with_input_delay(ggrs_config.input_delay)
            .with_fps(fps as usize)?
            .with_max_prediction_window(ggrs_config.max_prediction);

        let players = socket.players();
        let remote_peer = players.iter().find_map(|player| match player {
            PlayerType::Remote(peer) => Some(*peer),
            _ => None,
        });
        for (i, player) in players.into_iter().enumerate() {
            sess_build = sess_build.add_player(player, i)?;
        }

        let channel = socket
            .take_channel(0)
            .map_err(|e| anyhow::anyhow!("No data channel to the other player ({e:?})"))?;
        let p2p_session = sess_build.start_p2p_session(channel)?;
        //There should be only one.
        let local_player_index = *p2p_session.local_player_handles().first().unwrap_or(&0);

        let initial_state = match initial_state {
            Some(state) => state,
            //TODO: Actual input mapping. This will only be needed when it's a random match (we don't have an obvious P1 & P2)
            None => NetplayNesState::new(if local_player_index == 0 {
                JoypadMapping::P1
            } else {
                JoypadMapping::P2
            })?,
        };

        Ok(Self {
            p2p_session,
            netplay_server_configuration,
            local_player_index,

            older_confirmed_state: initial_state.clone(),
            newer_confirmed_state: initial_state.clone(),
            last_handled_ggrs_frame: -1,
            current_game_state: initial_state.clone(),
            start_time: Instant::now(),
            last_running_time: Instant::now(),
            catch_up_speed: 1.0,
            log: NetplayLog::new(),
            waited_frames: 0,
            lag: Recurrence::new(LAG_TIMES, LAG_WITHIN),
            last_lag: None,
            frame_time: 1.0 / fps,
            logged_frames_ahead: 0,
            socket,
            remote_peer,
            connection_path: None,
            _loop_task: loop_task,
        })
    }

    pub async fn advance(
        &mut self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        mut buffers: Option<NESBuffers<'_>>,
    ) {
        self.update_connection_path();
        let p2p_session = &mut self.p2p_session;

        match p2p_session.current_state() {
            ggrs::SessionState::Synchronizing => {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            ggrs::SessionState::Running => {
                self.last_running_time = Instant::now();
                for handle in p2p_session.local_player_handles() {
                    p2p_session
                        .add_local_input(handle, *joypad_state[0])
                        .expect("Handle to be a local player");
                }

                #[cfg(feature = "debug")]
                puffin::profile_scope!("ggrs advance_frame");
                match p2p_session.advance_frame() {
                    Ok(requests) => {
                        // Out of prediction window, GGRS returns no frame to advance, waiting for
                        // the other player's input. The game stands still meanwhile.
                        if requests
                            .iter()
                            .any(|request| matches!(request, GgrsRequest::AdvanceFrame { .. }))
                        {
                            if self.waited_frames > 0 {
                                log::debug!(
                                    "Waited {} frames for the other player's input, {} frames past the last confirmed one",
                                    self.waited_frames,
                                    p2p_session.max_prediction()
                                );
                                self.waited_frames = 0;
                            }
                        } else {
                            self.waited_frames += 1;
                            self.log.skipped += 1;
                            if self.waited_frames == 1 && self.lag.record(Instant::now()) {
                                self.last_lag = Some(Instant::now());
                            }
                        }
                        // CPU time spent re-simulating after a misprediction (not waiting for audio).
                        let mut rollback: Option<(i32, Duration)> = None;
                        for request in requests {
                            match request {
                                GgrsRequest::LoadGameState { cell, frame } => {
                                    let started = Instant::now();
                                    let depth = self.current_game_state.ggrs_frame - frame;
                                    self.current_game_state =
                                        cell.load().expect("ggrs state to load");
                                    rollback = Some((depth, started.elapsed()));
                                }
                                GgrsRequest::SaveGameState { cell, frame } => {
                                    assert_eq!(self.current_game_state.ggrs_frame, frame);
                                    cell.save(frame, Some(self.current_game_state.clone()), None);
                                }
                                GgrsRequest::AdvanceFrame { inputs } => {
                                    let is_replay = self.current_game_state.ggrs_frame
                                        <= self.last_handled_ggrs_frame;
                                    let started = Instant::now();

                                    // Set every frame, since a loaded state carries its own.
                                    self.current_game_state
                                        .nes_state
                                        .set_catch_up_speed(self.catch_up_speed);
                                    self.current_game_state
                                        .nes_state
                                        .advance(
                                            self.current_game_state.joypad_mapping.map(
                                                [
                                                    JoypadState(inputs[0].0),
                                                    JoypadState(inputs[1].0),
                                                ],
                                                self.local_player_index,
                                            ),
                                            if is_replay { None } else { buffers.take() },
                                            // GGRS steps exactly one NES frame at a time.
                                            1.0,
                                        )
                                        .await;

                                    if is_replay && let Some((_, spent)) = &mut rollback {
                                        *spent += started.elapsed();
                                    }
                                    if !is_replay {
                                        //This is not a replay
                                        self.last_handled_ggrs_frame =
                                            self.current_game_state.ggrs_frame;
                                        if self.current_game_state.ggrs_frame
                                            % (p2p_session.max_prediction() + 1) as i32
                                            == 0
                                        {
                                            mem::swap(
                                                &mut self.older_confirmed_state,
                                                &mut self.newer_confirmed_state,
                                            );
                                            self.newer_confirmed_state =
                                                self.current_game_state.clone()
                                        }
                                    }

                                    self.current_game_state.ggrs_frame += 1;
                                }
                            }
                        }
                        if let Some((depth, spent)) = rollback {
                            Self::log_rollback(depth, spent);
                            if depth >= LAG_ROLLBACK_FRAMES && self.lag.record(Instant::now()) {
                                self.last_lag = Some(Instant::now());
                            }
                            self.log.rollback(depth, spent);
                        }
                    }
                    Err(e) => {
                        self.log.skipped += 1;
                        log::warn!(
                            "Frame {} not advanced: {e:?}",
                            self.current_game_state.ggrs_frame,
                        )
                    }
                }

                let frames_ahead = p2p_session.frames_ahead();
                let stats = p2p_session.network_stats(1 - self.local_player_index).ok();
                self.update_catch_up_speed(frames_ahead);
                self.log.frame(stats, frames_ahead, self.catch_up_speed);
            }
        }
    }

    /// Slows down a little while ahead of the other player, and speeds up a little while behind,
    /// so the two meet in the middle. GGRS's `frames_ahead` is symmetric: the other player sees
    /// the same lead the other way round, so both steer, closing the gap twice as fast as one
    /// would and changing each one's speed (and pitch) half as much.
    ///
    /// GGRS averages `frames_ahead` over only half a second, so a network hiccup shows up as a
    /// big lead that's gone a second later. The speed eases towards its target (slowly away from
    /// full speed, quickly back) so a hiccup barely registers, while a lasting lead is still
    /// absorbed.
    fn update_catch_up_speed(&mut self, frames_ahead: i32) {
        let target = if frames_ahead.abs() >= CATCH_UP_MIN_FRAMES_AHEAD {
            //https://www.desmos.com/calculator/zbntsowijd
            let change = (NETPLAY_SPEED_CURVE_COEFF
                * (NETPLAY_SPEED_CURVE_SCALE * frames_ahead as f32).powi(2))
            .min(NETPLAY_MAX_SPEED_CHANGE);
            1.0 - change * frames_ahead.signum() as f32
        } else {
            1.0
        };
        let easing_time = if (target - 1.0).abs() > (self.catch_up_speed - 1.0).abs() {
            CATCH_UP_DEPART_TIME
        } else {
            CATCH_UP_RETURN_TIME
        };
        self.catch_up_speed +=
            (target - self.catch_up_speed) * (1.0 - (-self.frame_time / easing_time).exp());

        let was_apart = self.logged_frames_ahead.abs() >= CATCH_UP_MIN_FRAMES_AHEAD;
        if frames_ahead != self.logged_frames_ahead {
            if frames_ahead >= CATCH_UP_MIN_FRAMES_AHEAD {
                log::debug!(
                    "{frames_ahead} frames ahead of the other player, easing towards {target:.3}x (now {:.3}x)",
                    self.catch_up_speed
                );
            } else if frames_ahead <= -CATCH_UP_MIN_FRAMES_AHEAD {
                log::debug!(
                    "{} frames behind the other player, easing towards {target:.3}x (now {:.3}x)",
                    -frames_ahead,
                    self.catch_up_speed
                );
            } else if was_apart {
                log::debug!("Level with the other player, easing back to full speed");
            }
        }
        self.logged_frames_ahead = frames_ahead;
    }

    /// Rollbacks are normal; long ones are what makes netplay slow down, since the replay has to
    /// fit in a single frame next to the real one.
    fn log_rollback(depth: i32, spent: Duration) {
        const SLOW: Duration = Duration::from_millis(8); // Half a frame
        const DEEP: i32 = 4;
        let ms = spent.as_secs_f32() * 1000.0;
        if spent > SLOW {
            log::warn!("Rolling back {depth} frames took {ms:.1} ms of a 16.7 ms frame");
        } else if depth >= DEEP {
            log::debug!("Rolled back {depth} frames ({ms:.1} ms)");
        } else {
            log::trace!("Rolled back {depth} frames ({ms:.1} ms)");
        }
    }

    /// Picks up the candidate pair WebRTC settled on, which can change during the connection.
    fn update_connection_path(&mut self) {
        // Peer changes are GGRS's business; this only refreshes the candidate pairs.
        let _ = self.socket.try_update_peers();
        let Some(peer) = self.remote_peer else {
            return;
        };
        // Unknown until WebRTC reports the pair; is_relayed and candidate_pair come from the same pair.
        let (Some(relayed), Some(pair)) = (
            self.socket.is_relayed(peer),
            self.socket.candidate_pair(peer),
        ) else {
            return;
        };
        let path = if relayed {
            ConnectionPath::Relayed
        } else {
            ConnectionPath::Direct
        };
        if self.connection_path != Some(path) {
            log::info!(
                "Netplay connection to {peer} is {path:?} (local: {:?}, remote: {:?})",
                pair.local,
                pair.remote
            );
            self.connection_path = Some(path);
        }
    }

    fn to_shared_state(&self) -> SharedNetplayConnectedState {
        match self.p2p_session.current_state() {
            ggrs::SessionState::Synchronizing => SharedNetplayConnectedState::Synchronizing(
                self.last_running_time,
                self.netplay_server_configuration.unlock_url.clone(),
            ),
            ggrs::SessionState::Running => SharedNetplayConnectedState::Running(
                self.start_time,
                self.connection_path,
                self.last_lag,
            ),
        }
    }
}

pub struct ResumingNetplaySession {
    attempt1: ConnectingSession,
    attempt2: ConnectingSession,
    attempt1_failed: bool,
    attempt2_failed: bool,
}
impl ResumingNetplaySession {
    fn new(session_state: &mut ConnectedNetplaySession) -> Self {
        //TODO: Popup/info about the error? Or perhaps put the reason for the resume in the resume state below?
        log::debug!(
            "Resuming netplay to one of the frames {:?} and {:?}",
            session_state.older_confirmed_state.ggrs_frame,
            session_state.newer_confirmed_state.ggrs_frame
        );
        //let _ = connected.session_state.shared_state_sender.send(SharedNetplayState::Resuming);

        let netplay_server_configuration = &session_state.netplay_server_configuration;
        Self {
            attempt1: ConnectingSession::connect(StartMethod::Resume(
                netplay_server_configuration.clone(),
                Box::new(session_state.older_confirmed_state.clone()),
            )),
            attempt2: ConnectingSession::connect(StartMethod::Resume(
                netplay_server_configuration.clone(),
                Box::new(session_state.newer_confirmed_state.clone()),
            )),
            attempt1_failed: false,
            attempt2_failed: false,
        }
    }
}
#[derive(Debug)]
pub struct FailedNetplaySession {
    reason: String,
    unlock_url: Option<String>,
    pub start_method: StartMethod,
}
impl FailedNetplaySession {
    fn new(error: Error, start_method: StartMethod) -> Self {
        log::warn!("Netplay failed: {error:#}");
        Self {
            reason: error.to_string(),
            unlock_url: error
                .downcast_ref::<PeerUnreachable>()
                .and_then(|unreachable| unreachable.unlock_url.clone()),
            start_method,
        }
    }
}
pub enum NetplaySession {
    Disconnected(Box<LocalNesState>),
    Connecting(ConnectingSession),
    Connected(Box<ConnectedNetplaySession>),
    Resuming(ResumingNetplaySession),
    Failed(FailedNetplaySession),
}

impl NetplaySession {
    pub(crate) fn new(local_play_nes_state: LocalNesState) -> Self {
        Self::Disconnected(Box::new(local_play_nes_state))
    }

    pub(crate) fn start(start_method: StartMethod) -> NetplaySession {
        Self::Connecting(ConnectingSession::connect(start_method))
    }

    fn connect(connection: NetplayConnection, start_method: StartMethod) -> Self {
        match ConnectedNetplaySession::new(connection) {
            Ok(session) => Self::Connected(Box::new(session)),
            Err(e) => Self::fail(e.context("Could not start the game"), start_method),
        }
    }

    pub fn resume(session_state: &mut ConnectedNetplaySession) -> NetplaySession {
        Self::Resuming(ResumingNetplaySession::new(session_state))
    }
    fn fail(error: Error, start_method: StartMethod) -> Self {
        Self::Failed(FailedNetplaySession::new(error, start_method))
    }

    pub(crate) fn to_shared_state(&self) -> SharedNetplayState {
        match self {
            NetplaySession::Disconnected(..) => SharedNetplayState::Disconnected,
            NetplaySession::Connecting(connecting_session) => {
                SharedNetplayState::Connecting(connecting_session.state.clone())
            }
            NetplaySession::Connected(connected_netplay_session) => {
                SharedNetplayState::Connected(connected_netplay_session.to_shared_state())
            }
            NetplaySession::Resuming(..) => SharedNetplayState::Resuming,
            NetplaySession::Failed(failed_session) => SharedNetplayState::Failed(
                failed_session.reason.clone(),
                failed_session.unlock_url.clone(),
            ),
        }
    }
}

const POLLING_TIMEOUT: Duration = Duration::from_millis(1);

/// Minimum emulation speed applied when this peer is ahead of the remote (95%). The audio has to
/// stretch as much, and tetanes allows its rate to move 5% at most.
/// The connection is laggy when the game freezes waiting for the other player's input, or rolls
/// back at least `LAG_ROLLBACK_FRAMES` (a visible correction), `LAG_TIMES` times within
/// `LAG_WITHIN`. Once is a hiccup.
const LAG_TIMES: usize = 3;
const LAG_WITHIN: Duration = Duration::from_secs(30);
const LAG_ROLLBACK_FRAMES: i32 = 6;

/// The most catching up changes the speed, either way (0.98x to 1.02x). The audio is stretched
/// to match, which shifts its pitch by as much: 2% is about a third of a semitone, hard to hear,
/// where 5% sounded like a slowing tape. Both players steer, so a gap still closes at up to 4%.
const NETPLAY_MAX_SPEED_CHANGE: f32 = 0.02;
/// A one-frame lead comes and goes all the time and only means 0.4% slower; ignore it.
const CATCH_UP_MIN_FRAMES_AHEAD: i32 = 2;
/// How fast the catch-up speed follows its target (time constants, in seconds). Moving away from
/// full speed is gradual so a brief network hiccup barely changes it; returning is quicker.
const CATCH_UP_DEPART_TIME: f32 = 1.0;
const CATCH_UP_RETURN_TIME: f32 = 0.3;
/// Input scale for the quadratic slowdown curve: controls how quickly the curve
/// steepens as frames-ahead grows.
/// See the interactive graph: https://www.desmos.com/calculator/zbntsowijd
const NETPLAY_SPEED_CURVE_SCALE: f32 = 0.2;
/// Strength coefficient for the quadratic term of the slowdown curve.
const NETPLAY_SPEED_CURVE_COEFF: f32 = 0.1;
impl NetplaySession {
    pub async fn advance(
        &mut self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        buffers: Option<NESBuffers<'_>>,
        speed: f32,
    ) {
        match self {
            NetplaySession::Disconnected(local_nes_state) => {
                local_nes_state.advance(joypad_state, buffers, speed).await;
            }
            NetplaySession::Connecting(connecting_session) => {
                let future_connection = &mut connecting_session.netplay_connection;

                select! {
                    _ = tokio::time::sleep(POLLING_TIMEOUT) => {},
                    connection_result = future_connection => {
                        match connection_result {
                            Ok(connection) => {
                                *self = NetplaySession::connect(
                                    connection,
                                    connecting_session.start_method.clone(),
                                );
                            },
                            Err(e) => {
                                *self = NetplaySession::fail(e, connecting_session.start_method.clone());
                            },
                        }

                    }
                };
            }
            NetplaySession::Connected(session_state) => {
                let p2p_session = &mut session_state.p2p_session;
                p2p_session.poll_remote_clients();

                if p2p_session
                    .events()
                    .any(|e| matches!(e, ggrs::GgrsEvent::Disconnected { .. }))
                {
                    log::warn!("Peer disconnected, resuming...");
                    *self = NetplaySession::resume(session_state);
                } else {
                    session_state.advance(joypad_state, buffers).await;
                }
            }
            NetplaySession::Resuming(resuming) => {
                enum ResumeOutcome {
                    Connected(Box<NetplayConnection>),
                    Attempt1Failed,
                    Attempt2Failed,
                    Timeout,
                }
                let outcome = tokio::select! {
                    _ = tokio::time::sleep(POLLING_TIMEOUT) => ResumeOutcome::Timeout,
                    result = &mut resuming.attempt1.netplay_connection, if !resuming.attempt1_failed => {
                        match result {
                            Ok(c) => ResumeOutcome::Connected(Box::new(c)),
                            Err(_) => ResumeOutcome::Attempt1Failed,
                        }
                    }
                    result = &mut resuming.attempt2.netplay_connection, if !resuming.attempt2_failed => {
                        match result {
                            Ok(c) => ResumeOutcome::Connected(Box::new(c)),
                            Err(_) => ResumeOutcome::Attempt2Failed,
                        }
                    }
                };
                match outcome {
                    ResumeOutcome::Connected(c) => {
                        let start_method = resuming.attempt1.start_method.clone();
                        *self = NetplaySession::connect(*c, start_method);
                    }
                    ResumeOutcome::Attempt1Failed => {
                        resuming.attempt1_failed = true;
                        if resuming.attempt2_failed {
                            let start_method = resuming.attempt1.start_method.clone();
                            *self = NetplaySession::fail(
                                anyhow::anyhow!("Failed to resume connection"),
                                start_method,
                            );
                        }
                    }
                    ResumeOutcome::Attempt2Failed => {
                        resuming.attempt2_failed = true;
                        if resuming.attempt1_failed {
                            let start_method = resuming.attempt1.start_method.clone();
                            *self = NetplaySession::fail(
                                anyhow::anyhow!("Failed to resume connection"),
                                start_method,
                            );
                        }
                    }
                    ResumeOutcome::Timeout => {}
                }
            }
            NetplaySession::Failed(..) => {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
    }

    pub fn reset(&mut self, hard: bool) {
        if let NetplaySession::Disconnected(local_nes_state) = self {
            local_nes_state.reset(hard);
        }
    }

    pub fn save_sram(&mut self) -> Option<&[u8]> {
        if let NetplaySession::Disconnected(local_nes_state) = self {
            local_nes_state.save_sram()
        } else {
            None
        }
    }

    pub fn pace(&self) -> f32 {
        match self {
            NetplaySession::Connected(session) => session.catch_up_speed,
            _ => 1.0,
        }
    }

    pub fn frame(&self) -> u32 {
        match self {
            NetplaySession::Connecting(..)
            | NetplaySession::Resuming(..)
            | NetplaySession::Failed(..) => 0,
            NetplaySession::Connected(s) => s.current_game_state.nes_state.frame(),
            NetplaySession::Disconnected(local_nes_state) => local_nes_state.frame(),
        }
    }

    pub fn region(&self) -> crate::emulation::NesRegion {
        match self {
            NetplaySession::Connected(s) => s.current_game_state.nes_state.region(),
            NetplaySession::Disconnected(local_nes_state) => local_nes_state.region(),
            // What runs next, a netplay game.
            NetplaySession::Connecting(..)
            | NetplaySession::Resuming(..)
            | NetplaySession::Failed(..) => Bundle::current().config.default_region().clone(),
        }
    }
}

impl Debug for ConnectedNetplaySession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectedNetplaySession")
            .field("local_player_index", &self.local_player_index)
            .field(
                "p2p_session.current_state()",
                &self.p2p_session.current_state(),
            )
            .field("last_handled_ggrs_frame", &self.last_handled_ggrs_frame)
            .field("current_game_state", &self.current_game_state)
            .field("older_confirmed_state", &self.older_confirmed_state)
            .field("newer_confirmed_state", &self.newer_confirmed_state)
            .field("start_time", &self.start_time)
            .finish()
    }
}

impl Debug for ResumingNetplaySession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResumingNetplaySession")
            .field("attempt1", &self.attempt1)
            .field("attempt2", &self.attempt2)
            .finish()
    }
}

impl Debug for NetplaySession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disconnected(arg0) => f.debug_tuple("Disconnected").field(&arg0).finish(),
            Self::Connecting(arg0) => f.debug_tuple("Connecting").field(&arg0).finish(),
            Self::Connected(arg0) => f.debug_tuple("Connected").field(arg0).finish(),
            Self::Resuming(arg0) => f.debug_tuple("Resuming").field(arg0).finish(),
            Self::Failed(arg0) => f.debug_tuple("Failed").field(arg0).finish(),
        }
    }
}

/// Logs, now and then, how netplay is doing: how much it rolls back, how the network looks to
/// GGRS, and how much it slows down for the other player.
struct NetplayLog {
    since: Instant,
    frames: u32,
    rollbacks: u32,
    rolled_back_frames: u32,
    deepest_rollback: i32,
    rollback_time: Duration,
    skipped: u32,
    ping: Option<(u128, u128)>,
    send_queue: usize,
    frames_ahead: (i32, i32),
    speed: (f32, f32),
}

impl NetplayLog {
    const INTERVAL: Duration = Duration::from_secs(5);

    fn new() -> Self {
        Self {
            since: Instant::now(),
            frames: 0,
            rollbacks: 0,
            rolled_back_frames: 0,
            deepest_rollback: 0,
            rollback_time: Duration::ZERO,
            skipped: 0,
            ping: None,
            send_queue: 0,
            frames_ahead: (0, 0),
            speed: (1.0, 1.0),
        }
    }

    fn rollback(&mut self, depth: i32, spent: Duration) {
        self.rollbacks += 1;
        self.rolled_back_frames += depth.max(0) as u32;
        self.deepest_rollback = self.deepest_rollback.max(depth);
        self.rollback_time += spent;
    }

    fn frame(&mut self, stats: Option<ggrs::NetworkStats>, frames_ahead: i32, speed: f32) {
        self.frames += 1;
        if let Some(stats) = stats {
            self.ping = Some(match self.ping {
                None => (stats.ping, stats.ping),
                Some((low, high)) => (low.min(stats.ping), high.max(stats.ping)),
            });
            self.send_queue = self.send_queue.max(stats.send_queue_len);
        }
        self.frames_ahead = (
            self.frames_ahead.0.min(frames_ahead),
            self.frames_ahead.1.max(frames_ahead),
        );
        self.speed = (self.speed.0.min(speed), self.speed.1.max(speed));

        if self.since.elapsed() >= Self::INTERVAL {
            log::debug!(
                "Netplay: {} frames, {} rollbacks ({} frames re-run, deepest {}, {:.1} ms), {} waiting for input, ping {}, send queue up to {}, {} to {} frames ahead, speed {:.3}x to {:.3}x",
                self.frames,
                self.rollbacks,
                self.rolled_back_frames,
                self.deepest_rollback,
                self.rollback_time.as_secs_f32() * 1000.0,
                self.skipped,
                self.ping
                    .map_or("unknown".to_string(), |(low, high)| format!(
                        "{low}-{high} ms"
                    )),
                self.send_queue,
                self.frames_ahead.0,
                self.frames_ahead.1,
                self.speed.0,
                self.speed.1,
            );
            *self = Self::new();
        }
    }
}
