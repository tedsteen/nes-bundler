#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(unsafe_code)]
#![deny(clippy::all)]

use std::sync::Arc;
use std::time::Duration;

use audio::gui::AudioGui;
use audio::Audio;
use bundle::Bundle;

use input::gui::InputsGui;
use input::sdl2_impl::Sdl2Gamepads;
use input::Inputs;
use integer_scaling::MINIMUM_INTEGER_SCALING_SIZE;
use main_view::MainView;
use nes_state::emulator::{Emulator, SAMPLE_RATE};

use nes_state::gui::EmulatorGui;
use nes_state::{NES_HEIGHT, NES_WIDTH_4_3};
use settings::gui::ToGuiEvent;
use window::create_window;
use window::egui_winit_wgpu::Renderer;
use winit::event::{Event, WindowEvent};
use winit::event_loop::EventLoop;

use crate::input::gamepad::ToGamepadEvent;

use crate::settings::gui::GuiEvent;
use crate::settings::Settings;

mod audio;
mod bundle;
mod fps;
mod input;
mod integer_scaling;
mod main_view;
mod nes_state;
#[cfg(feature = "netplay")]
mod netplay;
mod settings;
mod window;

#[tokio::main]
async fn main() {
    init_logger();

    #[cfg(feature = "netplay")]
    if std::env::args()
        .collect::<String>()
        .contains(&"--print-netplay-id".to_string())
    {
        if let Some(id) = &Bundle::current().config.netplay.netplay_id {
            println!("{id}");
        }
        std::process::exit(0);
    }

    log::info!("NES Bundler is starting!");

    if let Err(e) = run().await {
        log::error!("nes-bundler failed to run :(\n{:?}", e)
    }
    std::process::exit(0);
}

async fn run() -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    let window = Arc::new(create_window(
        &Bundle::current().config.name,
        MINIMUM_INTEGER_SCALING_SIZE,
        Size::new(NES_WIDTH_4_3, NES_HEIGHT),
        &event_loop,
    )?);

    // Needed because: https://github.com/libsdl-org/SDL/issues/5380#issuecomment-1071626081
    sdl2::hint::set("SDL_JOYSTICK_THREAD", "1");
    // TODO: Perhaps do this to fix this issue: https://github.com/libsdl-org/SDL/issues/7896#issuecomment-1616700934
    //sdl2::hint::set("SDL_JOYSTICK_RAWINPUT", "0");

    let sdl_context = sdl2::init().map_err(anyhow::Error::msg)?;
    let mut sdl_event_pump = sdl_context.event_pump().map_err(anyhow::Error::msg)?;

    let mut audio = Audio::new(
        &sdl_context,
        Duration::from_millis(Settings::current().audio.latency as u64),
        SAMPLE_RATE as u32,
    )?;
    let audio_tx = audio.stream.start()?;

    let inputs = Inputs::new(Sdl2Gamepads::new(
        sdl_context.game_controller().map_err(anyhow::Error::msg)?,
    ));
    let joypad_state = inputs.joypads.clone();

    let emulator = Emulator::new()?;
    let mut renderer = Renderer::new(window.clone()).await?;
    let mut main_view = MainView::new(
        &mut renderer,
        vec![
            Box::new(AudioGui::new(audio)),
            Box::new(InputsGui::new(inputs)),
            Box::new(EmulatorGui::new(emulator.nes_state.clone())),
        ],
    );

    let _ = emulator.start(main_view.frame_pool.clone(), audio_tx, joypad_state);

    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    event_loop.run(|winit_event, control_flow| {
        #[cfg(feature = "debug")]
        puffin::profile_function!("event_loop.run");

        if let Event::WindowEvent {
            event: window_event,
            ..
        } = &winit_event
        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("handle winit events");

            match window_event {
                WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                    control_flow.exit();
                }
                WindowEvent::RedrawRequested => {
                    // Windows needs this to not freeze the window when resizing or moving
                    #[cfg(windows)]
                    window.request_redraw();
                }
                window_event => match window_event {
                    WindowEvent::Resized(physical_size) => {
                        renderer.resize(*physical_size);
                    }
                    winit_window_event => {
                        if !renderer
                            .egui
                            .handle_input(&renderer.window, winit_window_event)
                            .consumed
                        {
                            if let Some(winit_gui_event) = &winit_window_event.to_gui_event() {
                                main_view.handle_event(winit_gui_event, &renderer.window);
                            }
                        }
                    }
                },
            }
        };

        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("poll and handle sdl events");
            for sdl_gui_event in sdl_event_pump
                .poll_iter()
                .flat_map(|e| e.to_gamepad_event())
                .map(GuiEvent::Gamepad)
            {
                main_view.handle_event(&sdl_gui_event, &renderer.window);
            }
        }

        #[cfg(feature = "debug")]
        puffin::profile_scope!("render");
        main_view.render(&mut renderer);
    })?;

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
                eprintln!("Could not open nes-bundler-log.txt for writing, {:?}", e);
                env_logger::init();
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
