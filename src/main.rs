#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(unsafe_code)]
#![deny(clippy::all)]

use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::Duration;

use crate::settings::gui::ToGuiEvent;
use crate::{input::gamepad::ToGamepadEvent, settings::gui::GuiEvent};

use anyhow::Result;
use audio::{Audio, AudioSender};
use bundle::Bundle;

use fps::RateCounter;

use gui::MainGui;

use input::sdl2_impl::Sdl2Gamepads;
use input::Inputs;
use nes_state::emulator::Emulator;

use nes_state::FrameData;
use sdl2::EventPump;
use window::egui_winit_wgpu::Renderer;
use window::{create_window, NESFrame, Size};
use winit::event::{Event, WindowEvent};
use winit::event_loop::EventLoop;

mod audio;
mod bundle;
mod fps;
mod gui;
mod input;
mod integer_scaling;
mod nes_state;
#[cfg(feature = "netplay")]
mod netplay;
mod settings;
mod window;

const NES_WIDTH: u32 = 256;
const NES_WIDTH_4_3: u32 = (NES_WIDTH as f32 * (4.0 / 3.0)) as u32;
const NES_HEIGHT: u32 = 240;

const MINIMUM_INTEGER_SCALING_SIZE: (u32, u32) = (1024, 720);

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

fn init(renderer: &mut Renderer) -> Result<(MainGui, EventPump, AudioSender)> {
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

    let emulator = Emulator::start()?;
    let audio_tx = audio.stream.start()?;

    let main_gui = MainGui::new(renderer, emulator, inputs, audio);
    Ok((main_gui, sdl_event_pump, audio_tx))
}

async fn run() -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    let window = Arc::new(create_window(
        &Bundle::current().config.name,
        Size::new(
            MINIMUM_INTEGER_SCALING_SIZE.0 as f64,
            MINIMUM_INTEGER_SCALING_SIZE.1 as f64,
        ),
        Size::new(NES_WIDTH_4_3 as f64, NES_HEIGHT as f64),
        &event_loop,
    )?);
    let mut renderer = Renderer::new(window.clone()).await.expect("TED");
    let (event_tx, event_rx) = channel();
    let _ = std::thread::Builder::new()
        .name("Emulator".into())
        .spawn(move || {
            let (mut main_gui, mut sdl_event_pump, audio_tx) = init(&mut renderer).expect("TODO");
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
                                if let Some(winit_gui_event) = winit_window_event.to_gui_event() {
                                    main_gui.handle_event(&winit_gui_event, &renderer.window);
                                }
                            }
                        }
                    }
                }

                use crate::nes_state::NesStateHandler;

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

    Ok(event_loop.run(|winit_event, control_flow| {
        if let Event::WindowEvent {
            event: window_event,
            ..
        } = &winit_event
        {
            match window_event {
                WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                    control_flow.exit();
                }
                WindowEvent::RedrawRequested => {
                    // Windows needs this to not freeze the window when resizing or moving
                    #[cfg(windows)]
                    window.request_redraw();
                }
                window_event => event_tx.send(window_event.clone()).expect("TODO"),
            }
        };
    })?)
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
