#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(unsafe_code)]
#![deny(clippy::all)]

use crate::bundle::Bundle;
use crate::settings::gui::ToGuiEvent;
use crate::window::create_display;
use crate::{
    input::gamepad::ToGamepadEvent,
    settings::gui::{Gui, GuiEvent},
};
use anyhow::{Context, Result};
use audio::Audio;

use gameloop::{GameLoop, Time};
use input::Inputs;
use nes_state::local::LocalNesState;
use nes_state::start_nes;

use game::Game;
use rusticnes_core::cartridge::mapper_from_file;
use sdl2::EventPump;
use settings::Settings;
use window::Size;
use winit::event_loop::ControlFlow;

mod audio;
mod bundle;
#[cfg(feature = "debug")]
mod debug;
mod game;
mod gameloop;
mod input;
mod integer_scaling;
mod nes_state;
#[cfg(feature = "netplay")]
mod netplay;
mod settings;
mod window;

type Fps = f32;
const FPS: Fps = 60.0;
const NES_WIDTH: u32 = 256;
const NES_WIDTH_4_3: u32 = (NES_WIDTH as f32 * (4.0 / 3.0)) as u32;
const NES_HEIGHT: u32 = 240;

const MINIMUM_INTEGER_SCALING_SIZE: (u32, u32) = (1024, 720);

fn main() {
    init_logger();
    log::info!("nes-bundler starting!");
    if let Err(e) = run() {
        log::error!("nes-bundler failed to run :(\n{:?}", e)
    }
}
enum QueuedEvent {
    SdlEvent(sdl2::event::Event),
    WinitEvent(winit::event::WindowEvent),
}

fn run() -> anyhow::Result<()> {
    let (mut game_loop, winit_event_loop, mut sdl_event_pump) = initialise()?;

    let mut queued_events: Vec<QueuedEvent> = vec![];

    winit_event_loop.set_control_flow(ControlFlow::Poll);
    winit_event_loop
        .run(move |winit_event, control_flow| {
            let mut should_update = false;

            queued_events.append(
                &mut sdl_event_pump
                    .poll_iter()
                    .map(QueuedEvent::SdlEvent)
                    .collect(),
            );

            match &winit_event {
                winit::event::Event::WindowEvent {
                    event: window_event,
                    ..
                } => {
                    queued_events.push(QueuedEvent::WinitEvent(window_event.clone()));

                    use winit::event::WindowEvent;
                    match window_event {
                        WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                            control_flow.exit();
                        }
                        winit::event::WindowEvent::Resized(physical_size) => {
                            game_loop.game.gl_window.resize(*physical_size);
                        }
                        _ => {}
                    }
                }
                winit::event::Event::AboutToWait => {
                    should_update = true;
                }
                winit::event::Event::LoopExiting => {
                    game_loop.game.gui.destroy();
                    return;
                }
                _ => {}
            }

            if should_update {
                game_loop.next_frame(
                    |game_loop| {
                        let game = &mut game_loop.game;

                        // Let egui consume its events
                        queued_events.retain(|event| match &event {
                            QueuedEvent::WinitEvent(window_event) => {
                                !game.gui.on_event(game.gl_window.window(), window_event)
                            }
                            _ => true,
                        });

                        // Let the game consume the rest of the events
                        queued_events.retain(|event| {
                            let event = match event {
                                QueuedEvent::SdlEvent(event) => {
                                    event.to_gamepad_event().map(GuiEvent::Gamepad)
                                }
                                QueuedEvent::WinitEvent(window_event) => {
                                    window_event.to_gui_event()
                                }
                            };
                            if let Some(event) = &event {
                                game.apply_gui_event(event);
                            }

                            false
                        });

                        if let Some(frame_data) = game.advance() {
                            let fps = frame_data.fps;
                            #[cfg(feature = "debug")]
                            let fps = if game.debug.override_fps {
                                game.debug.fps
                            } else {
                                fps
                            };

                            game.draw_frame(Some(&frame_data.video));
                            game.push_audio(&frame_data.audio, fps);
                            game_loop.set_updates_per_second(fps);
                        } else {
                            game.draw_frame(None);
                        }
                    },
                    |game_loop| {
                        let game = &mut game_loop.game;

                        if game.run_gui() {
                            game.settings.save(&game.settings_path);
                        }

                        unsafe {
                            use glow::HasContext as _;
                            game.gl_window.glow_context.clear(glow::COLOR_BUFFER_BIT);
                        }

                        game.gui.paint(game.gl_window.window());

                        game.gl_window.swap_buffers().unwrap();
                    },
                );
            }
        })
        .map_err(anyhow::Error::msg)
}

#[allow(clippy::type_complexity)]
fn initialise() -> Result<
    (
        GameLoop<Game, Time>,
        winit::event_loop::EventLoop<()>,
        EventPump,
    ),
    anyhow::Error,
> {
    let bundle = Bundle::load()?;

    let event_loop = winit::event_loop::EventLoopBuilder::with_user_event()
        .build()
        .expect("Could not create the event loop");

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

    let gl_window = create_display(
        &bundle.config.name,
        bundle.window_icon.clone(),
        Size::new(
            MINIMUM_INTEGER_SCALING_SIZE.0 as f64,
            MINIMUM_INTEGER_SCALING_SIZE.1 as f64,
        ),
        Size::new(NES_WIDTH_4_3 as f64, NES_HEIGHT as f64),
        &event_loop,
    )?;

    let egui_glow = egui_glow::EguiGlow::new(
        &event_loop,
        gl_window.glow_context.clone(),
        None,
        Some(gl_window.get_dpi()),
    );

    #[allow(unused_mut)] //Needed by the netplay feature
    let mut settings = Settings::load(
        &bundle.settings_path,
        bundle.config.default_settings.clone(),
    );

    // Needed because: https://github.com/libsdl-org/SDL/issues/5380#issuecomment-1071626081
    sdl2::hint::set("SDL_JOYSTICK_THREAD", "1");
    // TODO: Perhaps do this to fix this issue: https://github.com/libsdl-org/SDL/issues/7896#issuecomment-1616700934
    //sdl2::hint::set("SDL_JOYSTICK_RAWINPUT", "0");

    let sdl_context = sdl2::init().map_err(anyhow::Error::msg)?;
    let audio = Audio::new(&sdl_context, &settings)?;
    let rom = bundle.rom.clone();

    let start_new_nes = move || -> LocalNesState {
        start_nes(
            mapper_from_file(&rom)
                .map_err(anyhow::Error::msg)
                .context("Failed to load ROM")
                .unwrap(),
        )
    };

    #[cfg(feature = "netplay")]
    let start_new_nes = || -> netplay::NetplayStateHandler {
        netplay::NetplayStateHandler::new(
            Box::new(start_new_nes),
            &bundle,
            &mut settings.netplay_id,
        )
    };

    Ok((
        GameLoop::new(
            Game::new(
                Box::new(start_new_nes()),
                Gui::new(egui_glow),
                settings,
                audio,
                Inputs::new(
                    sdl_context.game_controller().map_err(anyhow::Error::msg)?,
                    bundle.config.default_settings.input.selected.clone(),
                ),
                gl_window,
                bundle.settings_path.clone(),
            ),
            FPS,
            0.08,
        ),
        event_loop,
        sdl_context.event_pump().map_err(anyhow::Error::msg)?,
    ))
}

static NTSC_PAL: [u8; 64 * 8 * 3] = *include_bytes!("../ntscpalette.pal");

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
