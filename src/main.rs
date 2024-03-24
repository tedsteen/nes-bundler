//#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(unsafe_code)]
#![deny(clippy::all)]

use crate::bundle::Bundle;
use crate::settings::gui::ToGuiEvent;

use crate::{input::gamepad::ToGamepadEvent, settings::gui::GuiEvent};
use audio::Audio;

use fps::RateCounter;

use gui::MainGui;
use input::sdl2_impl::Sdl2Gamepads;
use input::Inputs;
use nes_state::emulator::Emulator;
use ringbuf::HeapRb;

use settings::Settings;
use window::egui_winit_wgpu::VideoFramePool;
use window::{create_state, Size};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};

mod audio;
mod bundle;
mod fps;
mod gameloop;
mod gui;
mod input;
mod integer_scaling;
mod nes_state;
#[cfg(feature = "netplay")]
mod netplay;
mod settings;
mod window;

type Fps = f32;
const FPS: Fps = 3579545.5 / 227.333 / 262.0;
const NES_WIDTH: u32 = 256;
const NES_WIDTH_4_3: u32 = (NES_WIDTH as f32 * (4.0 / 3.0)) as u32;
const NES_HEIGHT: u32 = 240;

const MINIMUM_INTEGER_SCALING_SIZE: (u32, u32) = (1024, 720);

#[tokio::main]
async fn main() {
    init_logger();
    log::info!("nes-bundler starting!");

    if let Err(e) = run().await {
        log::error!("nes-bundler failed to run :(\n{:?}", e)
    }
    std::process::exit(0);
}

async fn run() -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    let bundle = Bundle::load()?;
    let video_frame_pool = VideoFramePool::new();

    let mut state = create_state(
        &bundle.config.name,
        Size::new(
            MINIMUM_INTEGER_SCALING_SIZE.0 as f64,
            MINIMUM_INTEGER_SCALING_SIZE.1 as f64,
        ),
        Size::new(NES_WIDTH_4_3 as f64, NES_HEIGHT as f64),
        &event_loop,
        video_frame_pool.clone(),
    )
    .await?;

    // Needed because: https://github.com/libsdl-org/SDL/issues/5380#issuecomment-1071626081
    sdl2::hint::set("SDL_JOYSTICK_THREAD", "1");
    // TODO: Perhaps do this to fix this issue: https://github.com/libsdl-org/SDL/issues/7896#issuecomment-1616700934
    //sdl2::hint::set("SDL_JOYSTICK_RAWINPUT", "0");

    let sdl_context = sdl2::init().map_err(anyhow::Error::msg)?;
    let mut sdl_event_pump = sdl_context.event_pump().map_err(anyhow::Error::msg)?;

    #[cfg(feature = "netplay")]
    if std::env::args()
        .collect::<String>()
        .contains(&"--print-netplay-id".to_string())
    {
        if let Some(id) = bundle.config.netplay.netplay_id {
            println!("{id}");
        }
        std::process::exit(0);
    }

    #[allow(unused_mut)] //Needed by the netplay feature
    let mut settings = Settings::load(
        &bundle.settings_path,
        bundle.config.default_settings.clone(),
    );

    //TODO: Figure out a good buffer here..
    let (audio_tx, audio_rx) = HeapRb::<f32>::new(1024 * 8).split();

    let audio = Audio::new(&sdl_context, settings.audio.clone(), audio_rx)?;

    let inputs = Inputs::new(
        Sdl2Gamepads::new(sdl_context.game_controller().map_err(anyhow::Error::msg)?),
        bundle.config.default_settings.input.selected.clone(),
    );
    let emulator = Emulator::new(
        &bundle,
        &mut settings,
        video_frame_pool,
        audio_tx,
        inputs.joypads.clone(),
    );

    let mut main_gui = MainGui::new(
        &state.egui.context,
        emulator.new_gui(),
        emulator,
        settings,
        audio,
        inputs,
        bundle.settings_path.clone(),
    );

    let mut rate_counter = RateCounter::new();
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop
        .run(|winit_event, control_flow| {
            if let Some(report) = rate_counter.tick("EPS").report() {
                println!("{report}");
            }
            ////println!("EVENT: {:?}", winit_event);
            let mut should_render = false;
            let window_event = match winit_event {
                Event::WindowEvent {
                    event: window_event,
                    ..
                } => {
                    match window_event {
                        WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                            control_flow.exit();
                            None
                        }
                        WindowEvent::RedrawRequested => {
                            // Windows needs this to not freeze the windown when resizing or moving
                            #[cfg(windows)]
                            state.window.request_redraw();

                            should_render = true;
                            None
                        }
                        winit::event::WindowEvent::Resized(physical_size) => {
                            state.resize(physical_size);
                            None
                        }
                        _ => {
                            if !state
                                .egui
                                .handle_input(&state.window, &window_event)
                                .consumed
                            {
                                Some(window_event)
                            } else {
                                None
                            }
                        }
                    }
                }
                winit::event::Event::AboutToWait => {
                    should_render = true;
                    None
                }

                Event::LoopExiting => None,
                _ => None,
            };

            let mut gui_events = Vec::new();
            for sdl_gui_event in sdl_event_pump
                .poll_iter()
                .flat_map(|e| e.to_gamepad_event())
                .map(GuiEvent::Gamepad)
            {
                gui_events.push(sdl_gui_event);
            }
            if let Some(window_event) = window_event {
                if let Some(winit_gui_event) = window_event.to_gui_event() {
                    gui_events.push(winit_gui_event);
                }
            }

            for gui_event in &gui_events {
                main_gui.handle_event(gui_event, &state.window);
            }

            if should_render {
                //println!("RENDER: {:?}", std::time::Instant::now());
                main_gui.render_gui(&mut state);
                //thread::sleep(std::time::Duration::from_millis(10));
            }
        })
        .map_err(anyhow::Error::msg)
}

fn init_logger() {
    // #[cfg(windows)]
    // {
    //     match std::fs::OpenOptions::new()
    //         .create(true)
    //         .write(true)
    //         .truncate(true)
    //         .open("nes-bundler-log.txt")
    //     {
    //         Ok(log_file) => {
    //             env_logger::Builder::from_env(env_logger::Env::default())
    //                 .target(env_logger::Target::Pipe(Box::new(log_file)))
    //                 .init();
    //         }
    //         Err(e) => {
    //             eprintln!("Could not open nes-bundler-log.txt for writing, {:?}", e);
    //             env_logger::init();
    //         }
    //     }
    // }
    // #[cfg(not(windows))]
    {
        env_logger::init();
    }
}
