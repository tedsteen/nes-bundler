use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::audio::Audio;
use crate::gui::MainGui;
use crate::input::sdl2_impl::Sdl2Gamepads;
use crate::input::Inputs;
use crate::settings::gui::GuiComponent;
use crate::settings::Settings;
use crate::window::egui_winit_wgpu::Renderer;
use anyhow::Result;
use sdl2::EventPump;

#[cfg(feature = "netplay")]
type StateHandler = crate::netplay::NetplayStateHandler;
#[cfg(not(feature = "netplay"))]
type StateHandler = crate::nes_state::LocalNesState;

pub struct Emulator {
    pub nes_state: StateHandler,
}
pub const SAMPLE_RATE: f32 = 44_100.0;

impl Emulator {
    pub fn new() -> Result<Emulator> {
        #[cfg(not(feature = "netplay"))]
        let nes_state =
            crate::nes_state::LocalNesState::start_rom(&crate::bundle::Bundle::current().rom)?;

        #[cfg(feature = "netplay")]
        let nes_state = crate::netplay::NetplayStateHandler::new()?;

        let this = Self { nes_state };

        Ok(this)
    }

    pub fn init(renderer: &mut Renderer, emulator: Self) -> Result<(MainGui, EventPump)> {
        // Needed because: https://github.com/libsdl-org/SDL/issues/5380#issuecomment-1071626081
        sdl2::hint::set("SDL_JOYSTICK_THREAD", "1");
        // TODO: Perhaps do this to fix this issue: https://github.com/libsdl-org/SDL/issues/7896#issuecomment-1616700934
        //sdl2::hint::set("SDL_JOYSTICK_RAWINPUT", "0");

        let sdl_context = sdl2::init().map_err(anyhow::Error::msg)?;
        let sdl_event_pump = sdl_context.event_pump().map_err(anyhow::Error::msg)?;

        let audio_latency = Duration::from_millis(Settings::current().audio.latency as u64);
        let audio = Audio::new(&sdl_context, audio_latency, SAMPLE_RATE as u32)?;

        let inputs = Inputs::new(Sdl2Gamepads::new(
            sdl_context.game_controller().map_err(anyhow::Error::msg)?,
        ));

        let main_gui = MainGui::new(renderer, emulator, inputs, audio);
        Ok((main_gui, sdl_event_pump))
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
        ui.label(format!(
            "Frame: {}",
            super::NesStateHandler::frame(&instance.nes_state)
        ));
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
            netplay_gui: crate::netplay::gui::NetplayGui::new(),
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
