use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use crate::audio::{Audio, AudioSender};
use crate::bundle::Bundle;
use crate::fps::RateCounter;
use crate::gui::MainGui;
use crate::input::gamepad::ToGamepadEvent;
use crate::input::sdl2_impl::Sdl2Gamepads;
use crate::input::Inputs;
use crate::settings::gui::{GuiComponent, GuiEvent, ToGuiEvent};
use crate::window::egui_winit_wgpu::Renderer;
use crate::window::NESFrame;
use anyhow::Result;
use sdl2::EventPump;
use winit::event::WindowEvent;

use crate::nes_state::{FrameData, NesStateHandler};

#[cfg(feature = "netplay")]
type StateHandler = crate::netplay::NetplayStateHandler;
#[cfg(not(feature = "netplay"))]
type StateHandler = crate::nes_state::LocalNesState;

pub struct Emulator {
    pub nes_state: StateHandler,
}
impl Emulator {
    pub async fn start(window: Arc<winit::window::Window>) -> Result<Sender<WindowEvent>> {
        #[cfg(not(feature = "netplay"))]
        let nes_state = crate::nes_state::LocalNesState::start_rom(&Bundle::current().rom)?;

        #[cfg(feature = "netplay")]
        let nes_state = crate::netplay::NetplayStateHandler::new()?;

        let this = Self { nes_state };

        let mut renderer = Renderer::new(window.clone()).await?;

        let (event_tx, event_rx) = channel();
        let _ = std::thread::Builder::new()
            .name("Emulator".into())
            .spawn(move || {
                let (mut main_gui, mut sdl_event_pump, audio_tx) =
                    Self::init(&mut renderer, this).expect("the emulator to be able to initialise");
                let mut nes_frame = NESFrame::new();
                let mut rate_counter = RateCounter::new();

                loop {
                    rate_counter.tick("Loop");
                    puffin::GlobalProfiler::lock().new_frame();
                    #[cfg(feature = "debug")]
                    puffin::profile_function!("Render");

                    for sdl_gui_event in sdl_event_pump
                        .poll_iter()
                        .flat_map(|e| e.to_gamepad_event())
                        .map(GuiEvent::Gamepad)
                    {
                        main_gui.handle_event(&sdl_gui_event, &renderer.window);
                    }

                    for winit_window_event in event_rx.try_iter() {
                        match &winit_window_event {
                            WindowEvent::Resized(physical_size) => {
                                renderer.resize(*physical_size);
                            }
                            winit_window_event => {
                                if !renderer
                                    .egui
                                    .handle_input(&renderer.window, winit_window_event)
                                    .consumed
                                {
                                    if let Some(winit_gui_event) = winit_window_event.to_gui_event()
                                    {
                                        main_gui.handle_event(&winit_gui_event, &renderer.window);
                                    }
                                }
                            }
                        }
                    }

                    let joypads = &main_gui.inputs.joypads;
                    {
                        #[cfg(feature = "debug")]
                        puffin::profile_scope!("advance");
                        let mut frame_data = main_gui
                            .emulator
                            .nes_state
                            .advance(*joypads, &mut Some(&mut nes_frame));
                        {
                            #[cfg(feature = "debug")]
                            puffin::profile_scope!("push audio");
                            if let Some(FrameData { audio }) = &mut frame_data {
                                log::trace!("Pushing {:} audio samples", audio.len());
                                for s in audio {
                                    let _ = audio_tx.send(*s);
                                }
                            }
                        }
                    }
                    {
                        rate_counter.tick("Render");
                        #[cfg(feature = "debug")]
                        puffin::profile_scope!("render");
                        main_gui.render_gui(&mut renderer, &nes_frame);
                    }
                    if let Some(report) = rate_counter.report() {
                        println!("{report}");
                    }
                }
            });
        Ok(event_tx)
    }

    fn init(renderer: &mut Renderer, emulator: Self) -> Result<(MainGui, EventPump, AudioSender)> {
        // Needed because: https://github.com/libsdl-org/SDL/issues/5380#issuecomment-1071626081
        sdl2::hint::set("SDL_JOYSTICK_THREAD", "1");
        // TODO: Perhaps do this to fix this issue: https://github.com/libsdl-org/SDL/issues/7896#issuecomment-1616700934
        //sdl2::hint::set("SDL_JOYSTICK_RAWINPUT", "0");

        let sdl_context = sdl2::init().map_err(anyhow::Error::msg)?;
        let sdl_event_pump = sdl_context.event_pump().map_err(anyhow::Error::msg)?;

        //TODO: Figure out a resonable latency
        let mut audio = Audio::new(&sdl_context, Duration::from_millis(40), 44100)?;

        let inputs = Inputs::new(Sdl2Gamepads::new(
            sdl_context.game_controller().map_err(anyhow::Error::msg)?,
        ));

        let audio_tx = audio.stream.start()?;

        let main_gui = MainGui::new(renderer, emulator, inputs, audio);
        Ok((main_gui, sdl_event_pump, audio_tx))
    }

    pub fn save_state(&self) -> Option<Vec<u8>> {
        self.nes_state.save()
    }

    pub fn load_state(&mut self, data: &mut Vec<u8>) {
        self.nes_state.load(data);
    }

    pub fn emulation_speed() -> &'static Mutex<f32> {
        static MEM: OnceLock<Mutex<f32>> = OnceLock::new();
        MEM.get_or_init(|| Mutex::new(1_f32))
    }
}

#[cfg(feature = "debug")]
pub struct DebugGui {
    pub speed: f32,
    pub override_speed: bool,
}

pub struct EmulatorGui {
    #[cfg(feature = "netplay")]
    netplay_gui: crate::netplay::gui::NetplayGui,
    #[cfg(feature = "debug")]
    pub debug_gui: DebugGui,
}

#[cfg(feature = "debug")]
impl GuiComponent<Emulator> for DebugGui {
    fn ui(&mut self, instance: &mut Emulator, ui: &mut egui::Ui) {
        ui.label(format!("Frame: {}", instance.nes_state.frame()));
        ui.horizontal(|ui| {
            egui::Grid::new("debug_grid")
                .num_columns(2)
                .spacing([10.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    if ui
                        .checkbox(&mut self.override_speed, "Override emulation speed")
                        .changed()
                        && !self.override_speed
                    {
                        *Emulator::emulation_speed().lock().unwrap() = 1.0;
                    }

                    if self.override_speed {
                        ui.add(egui::Slider::new(&mut self.speed, 0.01..=2.0).suffix("x"));
                        *Emulator::emulation_speed().lock().unwrap() = self.speed;
                    }
                    ui.end_row();
                });
        });
    }
}

impl EmulatorGui {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "netplay")]
            netplay_gui: crate::netplay::gui::NetplayGui::new(
                Bundle::current().config.netplay.clone(),
            ),
            #[cfg(feature = "debug")]
            debug_gui: DebugGui {
                speed: 1.0,
                override_speed: false,
            },
        }
    }
}

impl GuiComponent<Emulator> for EmulatorGui {
    #[allow(unused_variables)]
    fn ui(&mut self, instance: &mut Emulator, ui: &mut egui::Ui) {
        #[cfg(feature = "debug")]
        self.debug_gui.ui(instance, ui);

        #[cfg(feature = "netplay")]
        self.netplay_gui.ui(&mut instance.nes_state, ui);
    }

    #[cfg(feature = "netplay")]
    fn messages(&self, instance: &Emulator) -> Option<Vec<String>> {
        self.netplay_gui.messages(&instance.nes_state)
    }

    fn name(&self) -> Option<String> {
        if cfg!(feature = "netplay") {
            #[cfg(feature = "netplay")]
            return self.netplay_gui.name();
        } else if cfg!(feature = "debug") {
            return Some("Debug".to_string());
        }

        None
    }

    #[cfg(feature = "netplay")]
    fn prepare(&mut self, instance: &mut Emulator) {
        self.netplay_gui.prepare(&mut instance.nes_state);
    }
}
