#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(unsafe_code)]
#![deny(clippy::all)]

use audio::gui::AudioGui;
use audio::Audio;
use bundle::Bundle;

use input::gamepad::ToGamepadEvent;
use input::gui::InputsGui;
use input::sdl2_impl::Sdl2Gamepads;
use input::{Inputs, JoypadState};
use main_view::MainView;

use settings::gui::GuiEvent;
use settings::{Settings, MAX_PLAYERS};

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use window::egui_winit_wgpu::Renderer;

use emulation::{BufferPool, Emulator, SAMPLE_RATE};
use integer_scaling::MINIMUM_INTEGER_SCALING_SIZE;

use emulation::{NES_HEIGHT, NES_WIDTH_4_3};
use window::create_window;
use winit::event::{Event, StartCause, WindowEvent};
use winit::event_loop::EventLoop;

mod audio;
mod bundle;
mod emulation;
mod fps;
mod gui;
mod input;
mod integer_scaling;
mod main_view;
#[cfg(feature = "netplay")]
mod netplay;
mod settings;
mod window;

#[tokio::main(worker_threads = 1)]
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

    let inputs = Inputs::new(Sdl2Gamepads::new(
        sdl_context.game_controller().map_err(anyhow::Error::msg)?,
    ));
    let audio_tx = audio.stream.start()?;

    let renderer = Renderer::new(window.clone()).await?;
    let mut main_view = MainView::new(renderer);
    let mut inputs_gui = InputsGui::new(inputs);
    let mut audio_gui = AudioGui::new(audio);

    let emulator = Emulator::new()?;
    let shared_inputs = Arc::new(RwLock::new([JoypadState(0); MAX_PLAYERS]));
    let frame_buffer = BufferPool::new();
    let mut emulator_gui = emulator
        .start_thread(audio_tx, shared_inputs.clone(), frame_buffer.clone())
        .await?;

    let mouse_hide_timeout = Duration::from_secs(1);
    let mut last_mouse_touch = Instant::now()
        .checked_sub(mouse_hide_timeout)
        .expect("there to be an instant `mouse_hide_timeout` seconds in the past");

    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    event_loop.run(|winit_event, control_flow| {
        let mut need_render = false;
        use crate::window::Fullscreen;
        match &winit_event {
            Event::NewEvents(StartCause::Init) => {
                if Bundle::current().config.start_in_fullscreen {
                    window.toggle_fullscreen();
                }
            }
            Event::WindowEvent {
                event: window_event,
                ..
            } => {
                match window_event {
                    WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                        control_flow.exit();
                    }
                    WindowEvent::RedrawRequested => {
                        // Windows needs this to not freeze the window when resizing or moving
                        #[cfg(windows)]
                        window.request_redraw();
                        need_render = true;
                    }
                    WindowEvent::MouseInput { .. } | WindowEvent::CursorMoved { .. } => {
                        last_mouse_touch = Instant::now();
                    }
                    _ => {}
                }
                main_view.handle_window_event(
                    window_event,
                    &mut audio_gui,
                    &mut inputs_gui,
                    &mut emulator_gui,
                );
            }
            Event::AboutToWait => {
                need_render = true;
            }
            _ => {}
        }

        window.set_cursor_visible(
            !(window.is_fullscreen()
                && !main_view.settings_gui.visible
                && Instant::now()
                    .duration_since(last_mouse_touch)
                    .gt(&mouse_hide_timeout)),
        );

        for sdl_gui_event in sdl_event_pump
            .poll_iter()
            .flat_map(|e| e.to_gamepad_event())
            .map(GuiEvent::Gamepad)
        {
            main_view.handle_gui_event(
                &sdl_gui_event,
                &mut audio_gui,
                &mut inputs_gui,
                &mut emulator_gui,
            );
        }

        let new_inputs = if !main_view.settings_gui.visible {
            inputs_gui.inputs.joypads
        } else {
            // Don't let the inputs control the game if the gui is showing
            [JoypadState(0), JoypadState(0)]
        };
        *shared_inputs.write().unwrap() = new_inputs;

        if need_render {
            main_view.render(
                &frame_buffer,
                &mut audio_gui,
                &mut inputs_gui,
                &mut emulator_gui,
            );
        }
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
