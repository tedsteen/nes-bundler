use std::{
    ops::{Deref, DerefMut},
    sync::{mpsc::Receiver, Arc, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard},
    time::Duration,
};

use anyhow::Result;
use sdl2::EventPump;
use serde::Deserialize;
use thingbuf::Recycle;
use tokio::task::JoinHandle;
use winit::event::WindowEvent;

use crate::{
    audio::{gui::AudioGui, Audio, AudioSender},
    fps::RateCounter,
    input::{
        gamepad::ToGamepadEvent, gui::InputsGui, sdl2_impl::Sdl2Gamepads, Inputs, JoypadState,
    },
    main_view::MainView,
    settings::{
        gui::{GuiComponent, GuiEvent},
        Settings, MAX_PLAYERS,
    },
    window::egui_winit_wgpu::Renderer,
};

pub mod gui;
pub mod tetanes;
use self::{gui::EmulatorGui, tetanes::TetanesNesState};
pub type LocalNesState = TetanesNesState;

pub const NES_WIDTH: u32 = 256;
pub const NES_WIDTH_4_3: u32 = (NES_WIDTH as f32 * (4.0 / 3.0)) as u32;
pub const NES_HEIGHT: u32 = 240;

static NTSC_PAL: &[u8] = include_bytes!("../../config/ntscpalette.pal");

#[cfg(feature = "netplay")]
pub type StateHandler = crate::netplay::NetplayStateHandler;
#[cfg(not(feature = "netplay"))]
pub type StateHandler = crate::emulation::LocalNesState;

pub struct Emulator {}
pub const SAMPLE_RATE: f32 = 44_100.0;

impl Emulator {
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }

    async fn init() -> Result<(EventPump, Inputs, Audio, AudioSender, StateHandler)> {
        #[cfg(not(feature = "netplay"))]
        let nes_state = crate::emulation::LocalNesState::start_rom(
            &crate::bundle::Bundle::current().rom,
            true,
        )?;

        #[cfg(feature = "netplay")]
        let nes_state = crate::netplay::NetplayStateHandler::new()?;

        // Needed because: https://github.com/libsdl-org/SDL/issues/5380#issuecomment-1071626081
        sdl2::hint::set("SDL_JOYSTICK_THREAD", "1");
        // TODO: Perhaps do this to fix this issue: https://github.com/libsdl-org/SDL/issues/7896#issuecomment-1616700934
        //sdl2::hint::set("SDL_JOYSTICK_RAWINPUT", "0");

        let sdl_context = sdl2::init().map_err(anyhow::Error::msg)?;
        let sdl_event_pump = sdl_context.event_pump().map_err(anyhow::Error::msg)?;

        let mut audio = Audio::new(
            &sdl_context,
            Duration::from_millis(Settings::current().audio.latency as u64),
            SAMPLE_RATE as u32,
        )?;

        let inputs = Inputs::new(Sdl2Gamepads::new(
            sdl_context.game_controller().map_err(anyhow::Error::msg)?,
        ));
        let audio_tx = audio.stream.start()?;
        Ok((sdl_event_pump, inputs, audio, audio_tx, nes_state))
    }
    pub async fn start_thread(
        &self,
        window: Arc<winit::window::Window>,
        event_rx: Receiver<WindowEvent>,
    ) -> Result<JoinHandle<()>> {
        let renderer = Renderer::new(window.clone()).await?;

        Ok(tokio::task::spawn_blocking(move || {
            let (mut sdl_event_pump, inputs, audio, audio_tx, nes_state) =
                futures::executor::block_on(Self::init()).expect("TODO");

            let mut main_view = MainView::new(renderer);
            let mut inputs_gui = InputsGui::new(inputs);
            let mut audio_gui = AudioGui::new(audio);
            let mut emulator_gui = EmulatorGui::new(nes_state);

            let mut audio_buffer = NESAudioFrame::new();
            let mut rate_counter = RateCounter::new();

            loop {
                #[cfg(feature = "debug")]
                puffin::profile_function!("Emulator loop");

                {
                    #[cfg(feature = "debug")]
                    puffin::profile_scope!("push audio");

                    log::trace!("Pushing {:} audio samples", audio_buffer.len());
                    for s in audio_buffer.iter() {
                        let _ = audio_tx.send(*s);
                    }
                }

                {
                    #[cfg(feature = "debug")]
                    puffin::profile_scope!("poll and handle events");

                    let gui_components: &mut [&mut dyn GuiComponent] =
                        &mut [&mut audio_gui, &mut inputs_gui, &mut emulator_gui];
                    for window_event in event_rx.try_iter() {
                        main_view.handle_window_event(&window_event, gui_components);
                    }

                    for sdl_gui_event in sdl_event_pump
                        .poll_iter()
                        .flat_map(|e| e.to_gamepad_event())
                        .map(GuiEvent::Gamepad)
                    {
                        main_view.handle_gui_event(&sdl_gui_event, gui_components);
                    }
                }

                {
                    #[cfg(feature = "debug")]
                    puffin::profile_scope!("advance");

                    rate_counter.tick("Frame");
                    audio_buffer.clear();
                    emulator_gui.netplay_gui.netplay_state_handler.advance(
                        inputs_gui.inputs.joypads,
                        &mut NESBuffers {
                            video: Some(&mut main_view.nes_frame),
                            audio: Some(&mut audio_buffer),
                        },
                    );
                }
                {
                    main_view.render(&mut [&mut audio_gui, &mut inputs_gui, &mut emulator_gui])
                }

                if let Some(report) = rate_counter.report() {
                    // Hitch-hike on the once-per-second-reporting to save the sram.
                    use base64::engine::general_purpose::STANDARD_NO_PAD as b64;
                    use base64::Engine;
                    Settings::current_mut().save_state = emulator_gui
                        .netplay_gui
                        .netplay_state_handler
                        .save_sram()
                        .map(|sram| b64.encode(sram));

                    log::debug!("Emulation: {report}");
                }
            }
        }))
    }

    fn _emulation_speed() -> &'static RwLock<f32> {
        static MEM: OnceLock<RwLock<f32>> = OnceLock::new();
        MEM.get_or_init(|| RwLock::new(1_f32))
    }

    pub fn emulation_speed<'a>() -> RwLockReadGuard<'a, f32> {
        Self::_emulation_speed().read().unwrap()
    }

    pub fn emulation_speed_mut<'a>() -> RwLockWriteGuard<'a, f32> {
        Self::_emulation_speed().write().unwrap()
    }
}

pub trait NesStateHandler {
    fn advance(&mut self, joypad_state: [JoypadState; MAX_PLAYERS], buffers: &mut NESBuffers);
    fn save_sram(&self) -> Option<Vec<u8>>;
    fn load_sram(&mut self, data: &mut Vec<u8>);
    fn frame(&self) -> u32;
}

#[derive(Deserialize, Debug)]
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
}

pub struct NESBuffers<'a> {
    pub audio: Option<&'a mut NESAudioFrame>,
    pub video: Option<&'a mut NESVideoFrame>,
}

pub struct NESVideoFrame(Vec<u8>);

impl NESVideoFrame {
    pub const SIZE: usize = (NES_WIDTH * NES_HEIGHT * 4) as usize;

    /// Allocate a new frame for video output.
    pub fn new() -> Self {
        let mut frame = vec![0; Self::SIZE];
        frame
            .iter_mut()
            .skip(3)
            .step_by(4)
            .for_each(|alpha| *alpha = 255);
        Self(frame)
    }
}

impl Default for NESVideoFrame {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for NESVideoFrame {
    type Target = Vec<u8>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NESVideoFrame {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct NESAudioFrame(Vec<f32>);
impl NESAudioFrame {
    fn new() -> NESAudioFrame {
        Self(Vec::new())
    }
}

impl Deref for NESAudioFrame {
    type Target = Vec<f32>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NESAudioFrame {
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
