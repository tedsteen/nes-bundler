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
mod recurrence;
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
            println!("{0}", turn_on_config.resolved_netplay_id());
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

/// Keeps macOS from slowing the app down once its window is in the background or minimized (App
/// Nap, timer coalescing): emulation and audio have to keep their pace, netplay most of all. The
/// returned activity lasts until it's dropped. Idle system sleep is still allowed.
#[cfg(target_os = "macos")]
fn keep_pace_in_background() -> impl Sized {
    use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};
    NSProcessInfo::processInfo().beginActivityWithOptions_reason(
        NSActivityOptions::UserInitiatedAllowingIdleSystemSleep
            | NSActivityOptions::LatencyCritical,
        &NSString::from_str("Running a game"),
    )
}

fn run() -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    let _activity = keep_pace_in_background();
    let app_context = AppContext::global();
    let event_loop = EventLoop::new()?;
    if app_context.config().enable_vsync.is_some() {
        log::warn!(
            "`enable_vsync` in config.yaml is no longer used: nes-bundler always syncs to the display"
        );
    }

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
        shared_state.emulator.clone(),
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
                    .format_timestamp_millis()
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
        env_logger::Builder::from_env(env_logger::Env::default())
            .format_timestamp_millis()
            .init();
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
