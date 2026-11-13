use std::{
    fmt::Debug,
    ops::{Deref, DerefMut},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering},
    },
};

use serde::{Deserialize, Serialize};

use thingbuf::{Recycle, ThingBuf};

use crate::{
    audio::{
        AudioStream,
        buffer::{AudioFlush, AudioProducer},
    },
    emulation::{clock::FrameClock, tetanes::TetanesNesState},
    input::JoypadState,
    settings::{MAX_PLAYERS, Settings},
};

pub mod clock;
pub mod gui;
pub mod tetanes;
pub type LocalNesState = TetanesNesState;
/// The bundle's game, in the player's region, with their battery save if `load_sram`.
pub fn new_local_nes_state(load_sram: bool) -> anyhow::Result<LocalNesState> {
    use anyhow::Context;
    // Bound out so the settings guard is released before `start_rom`, which takes one of its own
    // to restore SRAM.
    let region = Settings::current().nes_region();
    LocalNesState::start_rom(&crate::bundle::Bundle::current().rom, load_sram, &region)
        .context("The game's ROM could not be started")
}

/// Keeps the game's battery-backed RAM in the settings, which write it to disk in the background.
pub fn persist_sram<N: NesStateHandler>(nes_state: &mut N) {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD_NO_PAD as b64;
    if let Some(sram) = nes_state.save_sram() {
        let encoded = b64.encode(sram);
        Settings::current_mut().save_state = Some(encoded);
    }
}
pub const NES_WIDTH: u32 = 256;
pub const NES_HEIGHT: u32 = 240;

/// The bundle's palette: RGB for each of the 512 palette indices (64 colors × 8 emphasis
/// combinations).
pub static PALETTE: &[u8] = include_bytes!("../../config/palette.pal");

pub struct SharedEmulatorState {
    frame: AtomicU32,
    /// The region of the game running (see `NesRegion::to_u8`). Netplay runs the bundle's
    /// region whatever the player picked, so the display paces and shapes the picture by this.
    region: AtomicU8,
}
impl SharedEmulatorState {
    fn new() -> Self {
        Self {
            frame: AtomicU32::new(0),
            region: AtomicU8::new(Settings::current().nes_region().to_u8()),
        }
    }

    pub fn region(&self) -> NesRegion {
        NesRegion::from_u8(self.region.load(Ordering::Relaxed))
    }
}

#[derive(Clone)]
pub struct EmulatorControls {
    speed: Arc<AtomicU32>,
    inputs: Arc<[AtomicU8; MAX_PLAYERS]>,
    reset: Arc<AtomicU8>,
    shutdown: Arc<AtomicBool>,
    audio_flush: AudioFlush,
    clock: FrameClock,
}

impl EmulatorControls {
    fn new(audio_flush: AudioFlush) -> Self {
        Self {
            speed: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            inputs: Arc::new([AtomicU8::new(0), AtomicU8::new(0)]),
            reset: Arc::new(AtomicU8::new(Self::RESET_NONE)),
            shutdown: Arc::new(AtomicBool::new(false)),
            audio_flush,
            clock: FrameClock::default(),
        }
    }

    const RESET_NONE: u8 = 0;
    const RESET_SOFT: u8 = 1;
    const RESET_HARD: u8 = 2;

    #[cfg_attr(not(feature = "debug"), allow(dead_code))]
    pub fn set_speed(&self, speed: f32) {
        let speed = speed.to_bits();
        if self.speed.swap(speed, Ordering::Relaxed) != speed {
            self.audio_flush.flush();
        }
    }

    pub(crate) fn speed(&self) -> f32 {
        f32::from_bits(self.speed.load(Ordering::Relaxed))
    }

    pub fn write_inputs(&self, joypads: [JoypadState; MAX_PLAYERS]) {
        for (idx, joypad_state) in joypads.into_iter().enumerate() {
            self.inputs[idx].store(*joypad_state, Emulator::INPUT_WRITE_ORDERING);
        }
    }

    fn read_inputs(&self) -> [JoypadState; MAX_PLAYERS] {
        [
            JoypadState(self.inputs[0].load(Emulator::INPUT_READ_ORDERING)),
            JoypadState(self.inputs[1].load(Emulator::INPUT_READ_ORDERING)),
        ]
    }

    pub fn request_reset(&self, hard: bool) {
        let reset = if hard {
            Self::RESET_HARD
        } else {
            Self::RESET_SOFT
        };
        self.reset.fetch_max(reset, Ordering::Release);
        self.audio_flush.flush();
    }

    fn take_reset(&self) -> Option<bool> {
        match self.reset.swap(Self::RESET_NONE, Ordering::AcqRel) {
            Self::RESET_HARD => Some(true),
            Self::RESET_SOFT => Some(false),
            _ => None,
        }
    }

    pub fn request_shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
        self.clock.wake();
    }

    fn shutdown_requested(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }
}

#[derive(Clone)]
pub struct SharedEmulator {
    pub state: Arc<SharedEmulatorState>,
    controls: EmulatorControls,

    pub frame_buffer: VideoBufferPool,
}

#[derive(Clone)]
pub struct SharedState {
    pub emulator: SharedEmulator,
    #[cfg(feature = "netplay")]
    pub netplay: crate::netplay::SharedNetplay,
}

impl SharedEmulator {
    #[cfg_attr(not(feature = "debug"), allow(dead_code))]
    pub fn set_speed(&self, speed: f32) {
        self.controls.set_speed(speed);
    }

    pub fn write_inputs(&self, joypads: [JoypadState; MAX_PLAYERS]) {
        self.controls.write_inputs(joypads);
    }

    pub fn request_reset(&self, hard: bool) {
        self.controls.request_reset(hard);
    }

    /// Runs `frames` frames' worth of emulation, waiting up to `timeout` for it to finish. The
    /// display runs the game at `speed` times its own frame rate.
    pub fn tick(&self, frames: f32, speed: f32, timeout: std::time::Duration) {
        self.controls.clock.tick(frames, speed, timeout);
    }
}

impl SharedState {
    fn new(
        audio_flush: AudioFlush,
        #[cfg(feature = "netplay")] netplay: crate::netplay::SharedNetplay,
    ) -> Self {
        Self {
            emulator: SharedEmulator {
                state: Arc::new(SharedEmulatorState::new()),
                controls: EmulatorControls::new(audio_flush),
                frame_buffer: VideoBufferPool::new(),
            },

            #[cfg(feature = "netplay")]
            netplay,
        }
    }
}

pub struct Emulator {
    pub shared_state: SharedState,
    th: Option<std::thread::JoinHandle<()>>,
}

pub const DEFAULT_SAMPLE_RATE: f32 = 44_100.0;

struct EmulatorRuntime {
    frame_buffer: VideoBufferPool,
    shared_emulator: SharedEmulator,
    audio_producer: AudioProducer,
    #[cfg(feature = "netplay")]
    shared_netplay: crate::netplay::SharedNetplay,
    #[cfg(feature = "netplay")]
    netplay_state_sender: tokio::sync::watch::Sender<crate::netplay::SharedNetplayState>,
    #[cfg(feature = "netplay")]
    netplay_command_rx: tokio::sync::mpsc::Receiver<crate::netplay::NetplayCommand>,
}

impl EmulatorRuntime {
    fn spawn(self, local_nes_state: LocalNesState) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            let EmulatorRuntime {
                frame_buffer,
                shared_emulator,
                mut audio_producer,
                #[cfg(feature = "netplay")]
                shared_netplay,
                #[cfg(feature = "netplay")]
                netplay_state_sender,
                #[cfg(feature = "netplay")]
                netplay_command_rx,
            } = self;

            #[allow(unused_mut)]
            let mut nes_state = local_nes_state;
            let controls = shared_emulator.controls.clone();
            #[cfg(feature = "netplay")]
            let mut nes_state = crate::netplay::Netplay::new(
                nes_state,
                shared_netplay,
                netplay_state_sender,
                netplay_command_rx,
            );

            tokio::task::LocalSet::new().block_on(&rt, async move {
                // Frames owed but not run yet; fractions carry over to the next tick.
                let mut owed = 0.0f32;
                // Frames run on our own while the display stalled, which its next tick covers too.
                let mut ran_on_own = 0.0f32;
                let mut last_tick = std::time::Instant::now();
                let mut pacing = PacingLog::new();
                let mut last_frame = std::time::Instant::now();
                loop {
                    let region = nes_state.region();
                    shared_emulator
                        .state
                        .region
                        .store(region.to_u8(), Ordering::Relaxed);
                    let frame_time = 1.0 / region.to_fps();
                    // Step in once a tick is more than a frame and a quarter after the last one,
                    // before the audio runs low. The quarter is slack for a display a little slower
                    // than the game (60 Hz showing NTSC's 60.1) and for jitter; without it, a tick
                    // a fraction of a millisecond late counted as a stall. If the tick was only
                    // late, it covers what we ran and that's subtracted from it. On our own, check
                    // often, so each frame (and its audio) goes out close to when it's due rather
                    // than up to a frame late.
                    let deadline = if ran_on_own > 0.0 {
                        std::time::Instant::now() + std::time::Duration::from_millis(4)
                    } else {
                        last_tick + std::time::Duration::from_secs_f32(frame_time * 1.25)
                    };
                    let credit = tokio::time::timeout_at(deadline.into(), controls.clock.take()).await;
                    if controls.shutdown_requested() {
                        // Whatever was saved since the last snapshot.
                        persist_sram(&mut nes_state);
                        break;
                    }
                    if let Some(hard) = controls.take_reset() {
                        nes_state.reset(hard);
                    }

                    // Slower or faster while netplay meets the other player.
                    let pace = nes_state.pace();
                    match &credit {
                        Ok(credit) => {
                            if ran_on_own > 0.0 {
                                log::debug!(
                                    "Display ticking again; ran {ran_on_own:.1} frames on our own, the tick brought {:.1}",
                                    credit.frames * pace
                                );
                            }
                            owed += credit.frames * pace - std::mem::take(&mut ran_on_own);
                            audio_producer.set_display_speed(credit.speed);
                        }
                        // No tick: the display stalled (a hidden or minimized window, a window
                        // being dragged). Keep time on our own so the audio doesn't run dry.
                        Err(_) => {
                            if ran_on_own == 0.0 {
                                log::debug!("Display stopped ticking, keeping time on our own");
                            }
                            let due = last_tick.elapsed().as_secs_f32() / frame_time * pace;
                            audio_producer.set_display_speed(1.0);
                            owed += due;
                            ran_on_own += due;
                        }
                    }
                    last_tick = std::time::Instant::now();
                    owed = owed.max(-Emulator::MAX_OWED_FRAMES);
                    if owed > Emulator::MAX_OWED_FRAMES {
                        log::debug!("Fell {owed:.1} frames behind the display, skipping ahead");
                        owed = Emulator::MAX_OWED_FRAMES;
                    }
                    while owed >= 1.0 {
                        owed -= 1.0;
                        let started = std::time::Instant::now();
                        let gap = started.duration_since(last_frame);
                        if gap > std::time::Duration::from_millis(25) {
                            log::debug!(
                                "No frame for {:.1} ms (owed {:.2}, pace {pace:.3}, {})",
                                gap.as_secs_f32() * 1000.0,
                                owed + 1.0,
                                if credit.is_ok() { "display tick" } else { "on our own" }
                            );
                        }
                        Emulator::run_frame(
                            &mut nes_state,
                            &controls,
                            &frame_buffer,
                            &mut audio_producer,
                            &shared_emulator,
                        )
                        .await;
                        pacing.frames += 1;
                        last_frame = std::time::Instant::now();
                        let took = started.elapsed();
                        if took > std::time::Duration::from_millis(10) {
                            log::debug!("A frame took {:.1} ms", took.as_secs_f32() * 1000.0);
                        }
                    }
                    if let Ok(credit) = &credit {
                        controls.clock.finished(credit);
                    }
                    pacing.maybe_log(&audio_producer);
                }
            });
        })
    }
}

/// Logs, now and then, how well the emulator keeps pace with the display and the sound card.
struct PacingLog {
    since: std::time::Instant,
    frames: u32,
    underruns: u64,
    pushed: u64,
    played: u64,
}

impl PacingLog {
    const INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);

    fn new() -> Self {
        Self {
            since: std::time::Instant::now(),
            frames: 0,
            underruns: 0,
            pushed: 0,
            played: 0,
        }
    }

    fn maybe_log(&mut self, audio: &AudioProducer) {
        let elapsed = self.since.elapsed();
        if elapsed < Self::INTERVAL {
            return;
        }
        let stats = audio.stats();
        let per_second = |count: u64| count as f32 / elapsed.as_secs_f32();
        log::debug!(
            "Pacing: {:.2} frames/s, audio margin {:.1} ms (target {:.1} ms, sound card bursts up to {:.1} ms), rate {:+.2}% (drift {:+.3}%), {:.0} samples/s in, {:.0} out, {} underruns",
            self.frames as f32 / elapsed.as_secs_f32(),
            stats.margin_ms,
            stats.target_ms,
            stats.burst_ms,
            (stats.rate - 1.0) * 100.0,
            stats.drift * 100.0,
            per_second(stats.pushed - self.pushed),
            per_second(stats.played - self.played),
            stats.underruns - self.underruns,
        );
        *self = Self {
            underruns: stats.underruns,
            pushed: stats.pushed,
            played: stats.played,
            ..Self::new()
        };
    }
}

impl Emulator {
    /// Ordering used when the emulator thread *reads* the inputs written by the main thread.
    /// Paired with `Release` stores in `GameRuntime::write_inputs` to form a proper
    /// acquire-release pair, which is required for correctness on weak memory models (e.g. ARM).
    pub(crate) const INPUT_READ_ORDERING: std::sync::atomic::Ordering =
        std::sync::atomic::Ordering::Acquire;
    const INPUT_WRITE_ORDERING: std::sync::atomic::Ordering = std::sync::atomic::Ordering::Release;
    /// Ordering used when the emulator thread *writes* the frame counter read by other threads.
    const FRAME_WRITE_ORDERING: std::sync::atomic::Ordering = std::sync::atomic::Ordering::Release;
    const SRAM_SNAPSHOT_INTERVAL_FRAMES: u32 = 100;
    /// More owed than this (the emulator was stalled) is dropped rather than raced through. Room
    /// for a late tick's worth plus what's left over from the one before.
    const MAX_OWED_FRAMES: f32 = 5.0;

    async fn run_frame<N: NesStateHandler>(
        nes_state: &mut N,
        controls: &EmulatorControls,
        frame_buffer: &VideoBufferPool,
        audio: &mut AudioProducer,
        shared_emulator: &SharedEmulator,
    ) {
        // The newest frame replaces one the display hasn't picked up yet.
        let mut slot = frame_buffer.push_ref().ok().or_else(|| {
            drop(frame_buffer.pop_ref());
            frame_buffer.push_ref().ok()
        });
        nes_state
            .advance(
                controls.read_inputs(),
                Some(NESBuffers {
                    audio,
                    video: slot.as_deref_mut(),
                }),
                controls.speed(),
            )
            .await;
        drop(slot);

        let frame = nes_state.frame();
        shared_emulator
            .state
            .frame
            .store(frame, Emulator::FRAME_WRITE_ORDERING);

        if frame.is_multiple_of(Emulator::SRAM_SNAPSHOT_INTERVAL_FRAMES) {
            persist_sram(nes_state);
        }
    }

    pub fn new(audio_stream: &mut AudioStream, nes_state: LocalNesState) -> Self {
        let audio_producer = audio_stream.take_producer();
        let audio_flush = audio_producer.flush_handle();

        #[cfg(feature = "netplay")]
        let (shared_netplay, netplay_state_sender, netplay_command_rx) =
            crate::netplay::SharedNetplay::new();

        let shared_state = SharedState::new(
            audio_flush,
            #[cfg(feature = "netplay")]
            shared_netplay,
        );

        let runtime = EmulatorRuntime {
            frame_buffer: shared_state.emulator.frame_buffer.clone(),
            shared_emulator: shared_state.emulator.clone(),
            audio_producer,
            #[cfg(feature = "netplay")]
            shared_netplay: shared_state.netplay.clone(),
            #[cfg(feature = "netplay")]
            netplay_state_sender,
            #[cfg(feature = "netplay")]
            netplay_command_rx,
        };
        let thread_handle = runtime.spawn(nes_state);
        Self {
            shared_state,
            th: Some(thread_handle),
        }
    }
}

impl Drop for Emulator {
    fn drop(&mut self) {
        self.shared_state.emulator.controls.request_shutdown();
        if let Some(th) = self.th.take()
            && let Err(e) = th.join()
        {
            log::warn!("Failed to join emulator thread: {e:?}");
        }
    }
}

pub trait NesStateHandler {
    async fn advance(
        &mut self,
        joypad_state: [JoypadState; MAX_PLAYERS],
        buffers: Option<NESBuffers>,
        speed: f32,
    );
    fn reset(&mut self, hard: bool);
    fn save_sram(&mut self) -> Option<&[u8]>;
    fn frame(&self) -> u32;
    /// The region of the game running, which sets its frame rate and pixel shape.
    fn region(&self) -> NesRegion;
    /// How fast to run relative to the display; netplay slows down or speeds up to meet the other
    /// player.
    fn pace(&self) -> f32 {
        1.0
    }
}

#[derive(Clone, Serialize, Deserialize, Hash, Debug, PartialEq)]
pub enum NesRegion {
    Pal,
    Ntsc,
    Dendy,
}

impl NesRegion {
    pub fn to_fps(&self) -> f32 {
        match self {
            NesRegion::Pal => 50.006_977,
            NesRegion::Ntsc => 60.098_812,
            NesRegion::Dendy => 50.006_977,
        }
    }

    /// How wide an NES pixel is for each 1 tall, on the TV the console was made for: the ratio
    /// of the TV's square-pixel rate to the console's pixel rate. NTSC's is 8:7. PAL draws its
    /// pixels at a slower rate, so they're wider; Dendy shares PAL's clock.
    pub fn pixel_aspect_ratio(&self) -> f32 {
        match self {
            NesRegion::Ntsc => 8.0 / 7.0,
            NesRegion::Pal | NesRegion::Dendy => 2_950_000.0 / 2_128_137.0,
        }
    }

    fn to_u8(&self) -> u8 {
        match self {
            NesRegion::Pal => 0,
            NesRegion::Ntsc => 1,
            NesRegion::Dendy => 2,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            0 => NesRegion::Pal,
            2 => NesRegion::Dendy,
            _ => NesRegion::Ntsc,
        }
    }
}

pub struct NESBuffers<'a> {
    pub audio: &'a mut AudioProducer,
    pub video: Option<&'a mut NESVideoFrame>,
}

/// One NES frame as the PPU makes it: a palette index per pixel (a color and the emphasis bits).
/// The colors are looked up as it's drawn (see `nes_picture.wgsl`), which keeps that work off the
/// emulator thread and the upload half the size of RGBA.
pub struct NESVideoFrame(Vec<u16>);

impl NESVideoFrame {
    pub const SIZE: usize = (NES_WIDTH * NES_HEIGHT) as usize;

    /// Allocate a new frame for video output.
    pub fn new() -> Self {
        Self(vec![0; Self::SIZE])
    }
}

impl Default for NESVideoFrame {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for NESVideoFrame {
    type Target = Vec<u16>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NESVideoFrame {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug)]
pub struct FrameRecycle;

impl Recycle<NESVideoFrame> for FrameRecycle {
    fn new_element(&self) -> NESVideoFrame {
        NESVideoFrame::new()
    }

    fn recycle(&self, _frame: &mut NESVideoFrame) {}
}

#[derive(Debug)]
pub struct VideoBufferPool(Arc<ThingBuf<NESVideoFrame, FrameRecycle>>);

impl VideoBufferPool {
    pub fn new() -> Self {
        Self(Arc::new(ThingBuf::with_recycle(1, FrameRecycle)))
    }
}

impl Deref for VideoBufferPool {
    type Target = Arc<ThingBuf<NESVideoFrame, FrameRecycle>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Clone for VideoBufferPool {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}
