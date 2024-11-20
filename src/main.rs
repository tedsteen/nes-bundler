#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(unsafe_code)]
#![deny(clippy::all)]

use audio::gui::AudioGui;
use audio::Audio;
use bundle::Bundle;

use emulation::gui::EmulatorGui;
use futures::executor::block_on;
use input::gamepad::ToGamepadEvent;
use input::gui::InputsGui;
use input::sdl2_impl::Sdl2Gamepads;
use input::{Inputs, JoypadState};
use main_view::MainView;

use sdl2::EventPump;
use settings::{Settings, MAX_PLAYERS};
use winit::application::ApplicationHandler;
use winit::window::Window;

use crate::window::Fullscreen;
use emulation::{BufferPool, Emulator, EmulatorCommand, SAMPLE_RATE};
use integer_scaling::MINIMUM_INTEGER_SCALING_SIZE;
use std::sync::mpsc::Sender;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use window::egui_winit_wgpu::Renderer;

use emulation::{NES_HEIGHT, NES_WIDTH_4_3};
use window::create_window;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::EventLoop;

use crate::main_view::gui::GuiEvent;

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

type SharedInputs = Arc<RwLock<[JoypadState; MAX_PLAYERS]>>;

struct Application {
    window: Option<Arc<Window>>,
    main_view: Option<MainView>,

    last_mouse_touch: Instant,
    mouse_hide_timeout: Duration,
    audio_gui: AudioGui,
    inputs_gui: InputsGui,
    emulator_gui: EmulatorGui,
    sdl_event_pump: EventPump,
    shared_inputs: SharedInputs,
    frame_buffer: BufferPool,
    emulator_tx: Sender<EmulatorCommand>,
}
impl Application {
    async fn new(_event_loop: &EventLoop<()>) -> anyhow::Result<Self> {
        // Needed because: https://github.com/libsdl-org/SDL/issues/5380#issuecomment-1071626081
        sdl2::hint::set("SDL_JOYSTICK_THREAD", "1");
        // TODO: Perhaps do this to fix this issue: https://github.com/libsdl-org/SDL/issues/7896#issuecomment-1616700934
        //sdl2::hint::set("SDL_JOYSTICK_RAWINPUT", "0");

        let sdl_context = sdl2::init().map_err(anyhow::Error::msg)?;
        let sdl_event_pump = sdl_context.event_pump().map_err(anyhow::Error::msg)?;

        let mut audio = Audio::new(
            &sdl_context,
            Duration::from_millis(Settings::current().audio.latency as u64),
            SAMPLE_RATE as u32,
        )?;

        let inputs = Inputs::new(Sdl2Gamepads::new(
            sdl_context.game_controller().map_err(anyhow::Error::msg)?,
        ));
        let audio_tx = audio.stream.start()?;

        let inputs_gui = InputsGui::new(inputs);
        let audio_gui = AudioGui::new(audio);

        let emulator = Emulator::new()?;
        let shared_inputs = Arc::new(RwLock::new([JoypadState(0); MAX_PLAYERS]));
        let frame_buffer = BufferPool::new();
        let (emulator_gui, emulator_tx) = emulator
            .start_thread(audio_tx, shared_inputs.clone(), frame_buffer.clone())
            .await?;

        let mouse_hide_timeout = Duration::from_secs(1);
        Ok(Self {
            window: None,
            main_view: None,
            mouse_hide_timeout,
            last_mouse_touch: Instant::now()
                .checked_sub(mouse_hide_timeout)
                .expect("there to be an instant `mouse_hide_timeout` seconds in the past"),
            audio_gui,
            inputs_gui,
            emulator_gui,
            sdl_event_pump,
            shared_inputs,
            frame_buffer,
            emulator_tx,
        })
    }
}
impl Application {
    fn render(&mut self) {
        if let Some(main_view) = &mut self.main_view {
            main_view.render(
                &self.frame_buffer,
                &mut self.audio_gui,
                &mut self.inputs_gui,
                &mut self.emulator_gui,
            );
        }
    }
}
impl ApplicationHandler for Application {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window = create_window(
            &Bundle::current().config.name,
            MINIMUM_INTEGER_SCALING_SIZE,
            Size::new(NES_WIDTH_4_3, NES_HEIGHT),
            event_loop,
        )
        .expect("a window to be created");
        let window = Arc::new(window);

        let renderer = block_on(Renderer::new(window.clone())).expect("a renderer to be created");
        let main_view = MainView::new(renderer, self.emulator_tx.clone());
        self.main_view = Some(main_view);
        self.window = Some(window);
    }

    fn new_events(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop, cause: StartCause) {
        if let Some(window) = &self.window {
            if cause == StartCause::Init && Bundle::current().config.start_in_fullscreen {
                window.toggle_fullscreen();
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        if let Some(main_view) = &mut self.main_view {
            for sdl_gui_event in self
                .sdl_event_pump
                .poll_iter()
                .flat_map(|e| e.to_gamepad_event())
                .map(GuiEvent::Gamepad)
            {
                main_view.handle_gui_event(
                    &sdl_gui_event,
                    &mut self.audio_gui,
                    &mut self.inputs_gui,
                    &mut self.emulator_gui,
                );
            }
            let new_inputs = if !main_view.main_gui.visible() {
                self.inputs_gui.inputs.joypads
            } else {
                // Don't let the inputs control the game if the gui is showing
                [JoypadState(0), JoypadState(0)]
            };
            *self.shared_inputs.write().unwrap() = new_inputs;
        }

        self.render()
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        window_event: WindowEvent,
    ) {
        match window_event {
            WindowEvent::CloseRequested | WindowEvent::Destroyed => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                // Windows needs this to not freeze the window when resizing or moving
                #[cfg(windows)]
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
                self.render();
            }
            WindowEvent::MouseInput { .. } | WindowEvent::CursorMoved { .. } => {
                self.last_mouse_touch = Instant::now();
            }
            _ => {}
        }
        if let Some(main_view) = &mut self.main_view {
            main_view.handle_window_event(
                &window_event,
                &mut self.audio_gui,
                &mut self.inputs_gui,
                &mut self.emulator_gui,
            );
            if let Some(window) = &self.window {
                window.set_cursor_visible(
                    !(window.is_fullscreen()
                        && !main_view.main_gui.visible()
                        && Instant::now()
                            .duration_since(self.last_mouse_touch)
                            .gt(&self.mouse_hide_timeout)),
                );
            }
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let app = &mut Application::new(&event_loop).await?;

    event_loop.run_app(app)?;

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
