#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(unsafe_code)]
#![deny(clippy::all)]

use audio::gui::AudioGui;

use input::Inputs;
use input::gui::InputsGui;
use input::sdl3_impl::SDL3Gamepads;

use sdl3::EventPump;

use crate::app_context::AppContext;
use crate::app_shell::AppShell;
use crate::audio::AudioSystem;
use crate::emulation::gui::EmulatorGui;
use crate::game_runtime::GameRuntime;
use crate::ui_controller::UiController;
use winit::event_loop::EventLoop;

use crate::main_view::gui::MainGui;

mod app_context;
mod app_shell;
mod audio;
mod bundle;
mod emulation;
mod game_runtime;
mod gui;
mod input;
mod integer_scaling;
mod main_view;
#[cfg(feature = "netplay")]
mod netplay;
mod settings;
mod ui_controller;
mod window;

fn main() {
    init_logger();

    #[cfg(feature = "netplay")]
    if std::env::args().any(|arg| arg == "--print-netplay-id") {
        let app = AppContext::global();
        if let netplay::configuration::NetplayServerConfiguration::TurnOn(turn_on_config) =
            &app.config().netplay.server
        {
            println!("{0}", turn_on_config.get_netplay_id());
            std::process::exit(0);
        } else {
            eprintln!(
                "Netplay id not applicable for {0:#?}",
                app.config().netplay.server
            );
            std::process::exit(1);
        }
    }
    log::info!("NES Bundler is starting!");
    if let Err(e) = run() {
        log::error!("nes-bundler failed to run :(\n{:?}", e)
    }
}

fn run() -> anyhow::Result<()> {
    let app_context = AppContext::global();
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    // Needed because: https://github.com/libsdl-org/SDL/issues/5380#issuecomment-1071626081
    sdl3::hint::set("SDL_JOYSTICK_THREAD", "1");
    let sdl3_context = sdl3::init().map_err(anyhow::Error::msg)?;
    let sdl_event_pump: EventPump = sdl3_context.event_pump().map_err(anyhow::Error::msg)?;

    let audio_system = AudioSystem::new(sdl3_context.audio().expect("An SDL audio subsystem"));
    let settings = app_context.settings();
    let mut stream = audio_system.start_stream(settings);

    let runtime = GameRuntime::new(&mut stream);
    let shared_state = runtime.shared_state();

    let inputs = Inputs::new(SDL3Gamepads::new(
        sdl3_context.gamepad().map_err(anyhow::Error::msg)?,
    ));

    let main_gui = MainGui::new(
        shared_state.emulator.command_tx.clone(),
        AudioGui::new(audio_system.clone(), stream, settings),
        InputsGui::new(inputs, settings),
        EmulatorGui::new(shared_state),
        app_context.config().supported_nes_regions.clone(),
        settings,
    );
    let ui = UiController::new(main_gui, std::time::Duration::from_secs(1));
    let shell = &mut AppShell::new(app_context, runtime, sdl_event_pump, ui);
    event_loop.run_app(shell)?;

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

#[derive(Clone, Copy)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}
