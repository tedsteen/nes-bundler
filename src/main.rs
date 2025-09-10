#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(unsafe_code)]
#![deny(clippy::all)]

use audio::gui::AudioGui;
use bundle::Bundle;

use input::gamepad::ToGamepadEvent;
use input::gui::InputsGui;
use input::sdl3_impl::SDL3Gamepads;
use input::{Inputs, JoypadState};
use main_view::MainView;

use sdl3::EventPump;
use winit::application::ApplicationHandler;

use crate::audio::AudioSystem;
use crate::emulation::SharedEmulator;
use crate::emulation::gui::EmulatorGui;
use crate::window::Fullscreen;
use emulation::Emulator;
use integer_scaling::MINIMUM_INTEGER_SCALING_SIZE;
use std::time::{Duration, Instant};

use emulation::{NES_HEIGHT, NES_WIDTH_4_3};
use window::create_window;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::EventLoop;

use crate::main_view::gui::{GuiEvent, MainGui};

mod audio;
mod bundle;
mod emulation;
mod gui;
mod input;
mod integer_scaling;
mod main_view;
#[cfg(feature = "netplay")]
mod netplay;
mod settings;
mod window;

fn main() {
    init_logger();

    #[cfg(feature = "netplay")]
    if std::env::args()
        .collect::<String>()
        .contains(&"--print-netplay-id".to_string())
    {
        if let netplay::configuration::NetplayServerConfiguration::TurnOn(turn_on_config) =
            &Bundle::current().config.netplay.server
        {
            println!("{0}", turn_on_config.get_netplay_id());
            std::process::exit(0);
        } else {
            eprintln!(
                "Netplay id not applicable for {0:#?}",
                Bundle::current().config.netplay.server
            );
            std::process::exit(1);
        }
    }
    log::info!("NES Bundler is starting!");
    if let Err(e) = run() {
        log::error!("nes-bundler failed to run :(\n{:?}", e)
    }
    std::process::exit(0);
}

struct Application {
    main_view: Option<MainView>,

    last_mouse_touch: Instant,
    mouse_hide_timeout: Duration,
    shared_emulator: SharedEmulator,
    sdl_event_pump: EventPump,
    main_gui: MainGui,
}

impl Application {
    fn new(_event_loop: &EventLoop<()>) -> anyhow::Result<Self> {
        // Needed because: https://github.com/libsdl-org/SDL/issues/5380#issuecomment-1071626081
        sdl3::hint::set("SDL_JOYSTICK_THREAD", "1");

        let sdl3_context = sdl3::init().map_err(anyhow::Error::msg)?;
        let sdl_event_pump = sdl3_context.event_pump().map_err(anyhow::Error::msg)?;

        let audio_system = AudioSystem::new(sdl3_context.audio().expect("An SDL audio subsystem"));

        let mut stream = audio_system.start_stream();

        let emulator = Emulator::new(&mut stream);

        let inputs = Inputs::new(SDL3Gamepads::new(
            sdl3_context.gamepad().map_err(anyhow::Error::msg)?,
        ));

        let mouse_hide_timeout = Duration::from_secs(1);
        Ok(Self {
            main_view: None,
            mouse_hide_timeout,
            last_mouse_touch: Instant::now()
                .checked_sub(mouse_hide_timeout)
                .expect("there to be an instant `mouse_hide_timeout` seconds in the past"),
            shared_emulator: emulator.shared_state.emulator.clone(),
            sdl_event_pump,
            main_gui: MainGui::new(
                emulator.shared_state.emulator.command_tx.clone(),
                AudioGui::new(audio_system.clone(), stream),
                InputsGui::new(inputs),
                EmulatorGui::new(emulator.shared_state.clone()),
            ),
        })
    }
}

impl ApplicationHandler for Application {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window = create_window(
            &Bundle::current().config.name,
            MINIMUM_INTEGER_SCALING_SIZE,
            Size::new(NES_WIDTH_4_3, NES_HEIGHT),
            event_loop,
        )
        .expect("a window to be created");

        self.main_view = Some(MainView::new(
            window,
            self.shared_emulator.frame_buffer.clone(),
        ));
    }

    fn new_events(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop, cause: StartCause) {
        if let Some(main_view) = &self.main_view {
            if cause == StartCause::Init && Bundle::current().config.start_in_fullscreen {
                main_view.window.toggle_fullscreen();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        window_event: WindowEvent,
    ) {
        if let Some(main_view) = &mut self.main_view {
            match window_event {
                WindowEvent::CloseRequested | WindowEvent::Destroyed => event_loop.exit(),
                WindowEvent::RedrawRequested => {
                    main_view.render(&mut self.main_gui);
                    main_view.window.request_redraw();
                }
                WindowEvent::MouseInput { .. } | WindowEvent::CursorMoved { .. } => {
                    self.last_mouse_touch = Instant::now();
                }
                _ => {}
            }

            for sdl_gui_event in self
                .sdl_event_pump
                .poll_iter()
                .flat_map(|e| e.to_gamepad_event())
                .map(GuiEvent::Gamepad)
            {
                main_view.handle_gui_event(&sdl_gui_event, &mut self.main_gui);
            }
            let new_inputs = if !self.main_gui.visible() {
                self.main_gui.inputs_gui.inputs.joypads
            } else {
                // Don't let the inputs control the game if the gui is showing
                [JoypadState(0), JoypadState(0)]
            };
            self.shared_emulator.inputs[0]
                .store(*new_inputs[0], std::sync::atomic::Ordering::Relaxed);
            self.shared_emulator.inputs[1]
                .store(*new_inputs[1], std::sync::atomic::Ordering::Relaxed);

            main_view.handle_window_event(&window_event, &mut self.main_gui);
            main_view.window.set_cursor_visible(
                !(main_view.window.is_fullscreen()
                    && !self.main_gui.visible()
                    && Instant::now()
                        .duration_since(self.last_mouse_touch)
                        .gt(&self.mouse_hide_timeout)),
            );
        }
    }
}

fn run() -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let app = &mut Application::new(&event_loop)?;

    event_loop.run_app(app)?;

    Ok(())
}

fn init_logger() {
    #[cfg(windows)]
    {
        match std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open("nes-bundler-log.txt")
        {
            Ok(log_file) => {
                env_logger::Builder::from_env(env_logger::Env::default())
                    .target(env_logger::Target::Pipe(Box::new(log_file)))
                    .init();
            }
            Err(e) => {
                env_logger::init();
                log::warn!("Could not open nes-bundler-log.txt for writing, {:?}", e)
            }
        }
    }
    #[cfg(not(windows))]
    {
        env_logger::init();
    }
}

pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}
