#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(unsafe_code)]
#![deny(clippy::all)]

use std::sync::Arc;

use bundle::Bundle;

use fps::RateCounter;
use nes_state::emulator::Emulator;

use settings::gui::ToGuiEvent;
use window::egui_winit_wgpu::Renderer;
use window::{create_window, NESFrame, Size};
use winit::event::{Event, WindowEvent};
use winit::event_loop::EventLoop;

use crate::input::gamepad::ToGamepadEvent;
use crate::nes_state::{FrameData, NesStateHandler};
use crate::settings::gui::GuiEvent;
use crate::settings::Settings;

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
    #[cfg(feature = "debug")]
    puffin::set_scopes_on(true);

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
        Size::new(
            MINIMUM_INTEGER_SCALING_SIZE.0 as f64,
            MINIMUM_INTEGER_SCALING_SIZE.1 as f64,
        ),
        Size::new(NES_WIDTH_4_3 as f64, NES_HEIGHT as f64),
        &event_loop,
    )?);
    let emulator = Emulator::new()?;
    let mut renderer = Renderer::new(window.clone()).await?;

    let (mut main_gui, mut sdl_event_pump, audio_tx) =
        Emulator::init(&mut renderer, emulator).expect("the emulator to be able to initialise");
    let mut nes_frame = NESFrame::new();
    let mut rate_counter = RateCounter::new();

    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    event_loop.run(|winit_event, control_flow| {
        let mut render_needed = false;
        match &winit_event {
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
                        render_needed = true;
                    }
                    window_event => match &window_event {
                        WindowEvent::Resized(physical_size) => {
                            renderer.resize(*physical_size);
                            render_needed = true;
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
                    },
                }
            }
            Event::AboutToWait => {
                render_needed = true;
            }
            _ => {}
        };
        if render_needed {
            rate_counter.tick("Loop");
            #[cfg(feature = "debug")]
            puffin::profile_function!("Render");

            for sdl_gui_event in sdl_event_pump
                .poll_iter()
                .flat_map(|e| e.to_gamepad_event())
                .map(GuiEvent::Gamepad)
            {
                main_gui.handle_event(&sdl_gui_event, &renderer.window);
            }

            let joypads = &main_gui.inputs.joypads;
            let mut frame_data = {
                #[cfg(feature = "debug")]
                puffin::profile_scope!("advance");
                main_gui
                    .emulator
                    .nes_state
                    .advance(*joypads, &mut Some(&mut nes_frame))
            };
            {
                rate_counter.tick("Render");
                #[cfg(feature = "debug")]
                puffin::profile_scope!("render");
                main_gui.render_gui(&mut renderer, &nes_frame);
            }
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
            if let Some(report) = rate_counter.report() {
                // Hitch-hike on the one per second reporting to save the sram.
                use base64::engine::general_purpose::STANDARD_NO_PAD as b64;
                use base64::Engine;
                Settings::current().save_state = main_gui
                    .emulator
                    .nes_state
                    .save_sram()
                    .map(|s| b64.encode(s));
                log::debug!("{report}");
            }
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
