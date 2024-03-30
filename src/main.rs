#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(unsafe_code)]
#![deny(clippy::all)]

use std::sync::Arc;

use bundle::Bundle;

use nes_state::emulator::Emulator;

use window::{create_window, Size};
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
    let event_tx = Emulator::start(window.clone()).await?;

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
                window_event => {
                    let _ = event_tx.send(window_event.clone());
                }
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
