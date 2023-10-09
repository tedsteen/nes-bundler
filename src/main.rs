#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(unsafe_code)]
#![deny(clippy::all)]

use crate::bundle::{Bundle, LoadBundle};
use crate::input::buttons::GamepadButton;
use crate::nes_state::NesStateHandler;
use crate::settings::gui::ToGuiEvent;
use crate::window::{create_display, Fullscreen, GlutinWindowContext};
use crate::{
    gameloop::TimeTrait,
    input::{gamepad::ToGamepadEvent, KeyEvent},
    settings::gui::{Gui, GuiEvent},
};
use anyhow::Result;
use audio::Audio;
use egui::{Color32, ColorImage, ImageData};
use gameloop::{GameLoop, Time};

use base64::engine::general_purpose::STANDARD_NO_PAD as b64;
use base64::Engine;

use input::Inputs;
use nes_state::local::LocalNesState;
use nes_state::{start_nes, FrameData, get_mapper};
use palette::NTSC_PAL;

use sdl2::EventPump;
use settings::Settings;

mod audio;
mod bundle;
#[cfg(feature = "debug")]
mod debug;
mod gameloop;
mod input;
mod nes_state;
#[cfg(feature = "netplay")]
mod netplay;
mod palette;
mod settings;
mod window;

type Fps = f32;
const FPS: Fps = 60.0;
const WIDTH: u32 = 256;
const HEIGHT: u32 = 240;
const ZOOM: u8 = 3;

const DEFAULT_WINDOW_SIZE: (u32, u32) = (
    crate::WIDTH * crate::ZOOM as u32,
    crate::WIDTH * crate::ZOOM as u32,
);

fn main() {
    init_logger();

    log::info!("nes-bundler starting!");
    match initialise() {
        Ok((game_loop, event_loop, sdl_event_pump, gl_window)) => {
            run(game_loop, event_loop, sdl_event_pump, gl_window);
        }
        Err(e) => {
            log::error!("nes-bundler failed to start :(\n{:?}", e);
        }
    }
}

fn run(
    mut game_loop: GameLoop<Game, Time>,
    winit_event_loop: winit::event_loop::EventLoop<()>,
    mut sdl_event_pump: EventPump,
    mut gl_window: GlutinWindowContext,
) -> ! {
    winit_event_loop.run(move |winit_event, _, control_flow| {
        if log::max_level() == log::Level::Trace && Time::now().sub(&game_loop.last_stats) >= 1.0 {
            let (ups, rps, ..) = game_loop.get_stats();
            log::trace!("UPS: {:?}, RPS: {:?}", ups, rps);
        }

        let mut sdl_events: Vec<GuiEvent> = sdl_event_pump
            .poll_iter()
            .filter_map(|sdl_event| sdl_event.to_gamepad_event().map(GuiEvent::Gamepad))
            .collect();
        let gui = &mut game_loop.game.gui;
        match &winit_event {
            winit::event::Event::WindowEvent { event, .. } => {
                use winit::event::WindowEvent;
                if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed) {
                    *control_flow = winit::event_loop::ControlFlow::Exit;
                }

                if let winit::event::WindowEvent::Resized(physical_size) = &event {
                    gl_window.resize(*physical_size);
                } else if let winit::event::WindowEvent::ScaleFactorChanged {
                    new_inner_size, ..
                } = &event
                {
                    gl_window.resize(**new_inner_size);
                }

                if !gui.on_event(event) {
                    if let Some(winit_gui_event) = &event.to_gui_event() {
                        sdl_events.push(winit_gui_event.clone());
                    }
                }
            }
            winit::event::Event::LoopDestroyed => {
                gui.destroy();
                return;
            }
            _ => {}
        }

        for event in sdl_events {
            let game = &mut game_loop.game;
            let consumed = match &event {
                settings::gui::GuiEvent::Keyboard(KeyEvent::Pressed(key_code, modifiers)) => {
                    let settings = &mut game.settings;
                    let nes_state = &mut game.nes_state;

                    use crate::input::keys::KeyCode::*;
                    match key_code {
                        F1 => {
                            if let Some(save_state) = nes_state.save() {
                                settings.last_save_state = Some(b64.encode(save_state));
                                settings.save();
                            }
                            
                            true
                        }
                        F2 => {
                            if let Some(save_state) = &settings.last_save_state {
                                if let Ok(buf) = &mut b64.decode(save_state) {
                                    nes_state.load(buf);
                                }
                            }
                            true
                        }
                        Escape => {
                            game.gui.toggle_visibility();
                            true
                        }

                        key_code => gl_window
                            .window_mut()
                            .check_and_set_fullscreen(modifiers, key_code),
                    }
                }
                GuiEvent::Gamepad(input::gamepad::GamepadEvent::ButtonDown {
                    button: GamepadButton::Guide,
                    ..
                }) => {
                    game.gui.toggle_visibility();
                    true
                }
                _ => false,
            };
            if !consumed {
                game.apply_gui_event(event);
            }
        }

        game_loop.next_frame(
            |game_loop| {
                let game = &mut game_loop.game;
                #[allow(unused_mut)] //debug feature needs this
                if let Some(frame_data) = game.advance() {
                    let mut fps = frame_data.fps;
                    #[cfg(feature = "debug")]
                    if game.debug.override_fps {
                        fps = game.debug.fps;
                    }

                    game.draw_frame(Some(&frame_data.video));
                    game.push_audio(&frame_data.audio, fps);
                    game_loop.set_updates_per_second(fps);
                } else {
                    game.draw_frame(None);
                }
            },
            |game_loop| {
                let game = &mut game_loop.game;
                if let winit::event::Event::RedrawEventsCleared = &winit_event {
                    if game.run_gui(gl_window.window()) {
                        game.settings.save();
                    }

                    unsafe {
                        use glow::HasContext as _;
                        //gl.clear_color(clear_colour[0], clear_colour[1], clear_colour[2], 1.0);
                        gl_window.glow_context.clear(glow::COLOR_BUFFER_BIT);
                    }

                    // draw things behind egui here

                    game.gui.paint(gl_window.window());

                    // draw things on top of egui here

                    gl_window.swap_buffers().unwrap();
                }
            },
        );
    })
}

#[allow(clippy::type_complexity)]
fn initialise() -> Result<
    (
        GameLoop<Game, Time>,
        winit::event_loop::EventLoop<()>,
        EventPump,
        GlutinWindowContext,
    ),
    anyhow::Error,
> {
    sdl2::hint::set("SDL_JOYSTICK_THREAD", "1");
    let bundle = Bundle::load()?;
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
    let event_loop = winit::event_loop::EventLoopBuilder::with_user_event().build();
    let gl_window = create_display(
        &bundle.config.window_title,
        DEFAULT_WINDOW_SIZE.0,
        DEFAULT_WINDOW_SIZE.1,
        &event_loop,
    )?;

    let egui_glow = egui_glow::EguiGlow::new(&event_loop, gl_window.glow_context.clone(), None);
    egui_glow.egui_ctx.set_pixels_per_point(gl_window.get_dpi());

    #[allow(unused_mut)] //Needed by the netplay feature
    let mut settings = Settings::new(bundle.config.default_settings.clone());

    let sdl_context = sdl2::init().map_err(anyhow::Error::msg)?;
    let audio = Audio::new(&sdl_context, &settings)?;
    let mapper = get_mapper(&bundle)?;

    let start_new_nes = move || -> LocalNesState {
        start_nes(mapper.clone())
    };

    #[cfg(feature = "netplay")]
    #[allow(unused_mut)] //Bug, I had to make it mut
    let mut start_new_nes = || -> netplay::NetplayStateHandler {
        netplay::NetplayStateHandler::new(Box::new(start_new_nes), &bundle, &mut settings.netplay_id)
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
            ),
            FPS,
            0.08,
        ),
        event_loop,
        sdl_context.event_pump().map_err(anyhow::Error::msg)?,
        gl_window,
    ))
}

struct Game {
    nes_state: Box<dyn NesStateHandler>,
    gui: Gui,
    settings: Settings,
    #[cfg(feature = "debug")]
    debug: debug::Debug,
    audio: Audio,
    inputs: Inputs,
}
impl Game {
    pub fn new(
        nes_state: Box<dyn NesStateHandler>,
        gui: Gui,
        settings: Settings,
        audio: Audio,
        inputs: Inputs,
    ) -> Self {
        Self {
            nes_state,
            gui,
            inputs,
            settings,
            #[cfg(feature = "debug")]
            debug: debug::Debug::new(),
            audio,
        }
    }
    fn apply_gui_event(&mut self, gui_event: GuiEvent) {
        self.gui.handle_events(
            &gui_event,
            &mut [
                #[cfg(feature = "debug")]
                Some(&mut self.debug),
                Some(&mut self.audio),
                Some(&mut self.inputs),
                self.nes_state.get_gui(),
            ],
            &mut self.settings,
        )
    }

    fn run_gui(&mut self, window: &winit::window::Window) -> bool {
        let settings_hash_before = self.settings.get_hash();
        self.audio.sync_audio_devices(&mut self.settings.audio);

        self.gui.ui(
            window,
            &mut [
                #[cfg(feature = "debug")]
                Some(&mut self.debug),
                Some(&mut self.audio),
                Some(&mut self.inputs),
                self.nes_state.get_gui(),
            ],
            &mut self.settings,
        );
        settings_hash_before != self.settings.get_hash()
    }

    pub fn advance(&mut self) -> Option<FrameData> {
        self.nes_state
            .advance([self.inputs.get_joypad(0), self.inputs.get_joypad(1)])
    }

    pub fn draw_frame(&mut self, video_data: Option<&[u16]>) {
        let new_image_data = video_data.map(|frame| {
            let mut image_data = ImageData::Color(ColorImage::new(
                [WIDTH as usize, HEIGHT as usize],
                Color32::BLACK,
            ));
            if let ImageData::Color(color_data) = &mut image_data {
                for (i, pixel) in color_data.pixels.iter_mut().enumerate() {
                    let palette_index = frame[i] as usize * 4;
                    let color = &NTSC_PAL[palette_index..palette_index + 4];
                    *pixel =
                        Color32::from_rgba_premultiplied(color[0], color[1], color[2], color[3]);
                }
            }
            image_data
        });

        self.gui.update_nes_texture(new_image_data);
    }

    fn push_audio(&mut self, samples: &[i16], fps_hint: Fps) {
        self.audio.stream.push_samples(samples, fps_hint);
    }
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
