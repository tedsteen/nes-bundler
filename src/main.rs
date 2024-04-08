#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(unsafe_code)]
#![deny(clippy::all)]

use bundle::Bundle;
use std::sync::mpsc::channel;
use std::sync::Arc;

use emulation::Emulator;
use integer_scaling::MINIMUM_INTEGER_SCALING_SIZE;

use emulation::{NES_HEIGHT, NES_WIDTH_4_3};
use window::create_window;
use winit::event::{Event, WindowEvent};
use winit::event_loop::EventLoop;

mod audio;
mod bundle;
mod emulation;
mod fps;
mod input;
mod integer_scaling;
mod main_view;
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
    let emulator = Emulator::new()?;

    let (event_tx, event_rx) = channel();
    emulator.start_thread(window.clone(), event_rx).await?;

    event_loop.run(|winit_event, control_flow| {
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
                _ => {}
            }
            event_tx
                .send(window_event.clone())
                .expect("to be able to send the window event");
        };
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
