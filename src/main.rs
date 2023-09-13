#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(unsafe_code)]
#![deny(clippy::all)]

use crate::bundle::{Bundle, LoadBundle};
use crate::input::buttons::GamepadButton;
use crate::settings::gui::ToGuiEvent;
use crate::window::{create_display, Fullscreen, GlutinWindowContext};
use crate::{
    gameloop::TimeTrait,
    input::{gamepad::ToGamepadEvent, JoypadInput, KeyEvent},
    settings::gui::{Gui, GuiEvent},
};
use anyhow::{Context, Result};
use audio::Audio;
use egui::{Color32, ColorImage, ImageData};
use gameloop::{GameLoop, Time};
use settings::gui::{EmptyGuiComponent, GuiComponent};

use base64::engine::general_purpose::STANDARD_NO_PAD as b64;
use base64::Engine;

use input::{Input, Inputs};
use palette::NTSC_PAL;
use rusticnes_core::cartridge::mapper_from_file;
use rusticnes_core::nes::NesState;
use sdl2::{EventPump, Sdl};
use settings::{Settings, MAX_PLAYERS};

mod audio;
mod bundle;
#[cfg(feature = "debug")]
mod debug;
mod gameloop;
mod input;
#[cfg(feature = "netplay")]
mod netplay;
mod palette;
mod settings;
mod window;

type Fps = u32;
const FPS: Fps = 60;
const WIDTH: u32 = 256;
const HEIGHT: u32 = 240;
const ZOOM: u8 = 3;

const DEFAULT_WINDOW_SIZE: (u32, u32) = (
    crate::WIDTH * crate::ZOOM as u32,
    crate::WIDTH * crate::ZOOM as u32,
);

pub fn start_nes(cart_data: Vec<u8>, sample_rate: u64) -> Result<NesState> {
    let rom_data = match std::env::var("ROM_FILE") {
        Ok(rom_file) => {
            std::fs::read(&rom_file).context(format!("Could not read ROM {}", rom_file))?
        }
        Err(_e) => cart_data.to_vec(),
    };

    let mapper = mapper_from_file(rom_data.as_slice())
        .map_err(anyhow::Error::msg)
        .context("Failed to load ROM")?;
    #[cfg(feature = "debug")]
    mapper.print_debug_status();
    let mut nes = NesState::new(mapper);
    nes.power_on();
    nes.apu.set_sample_rate(sample_rate);

    Ok(nes)
}

fn main() {
    init_logger();

    log::info!("nes-bundler starting!");
    match initialise() {
        Ok((game_loop, event_loop, sdl_event_pump)) => {
            run(game_loop, event_loop, sdl_event_pump);
        }
        Err(e) => {
            log::error!("nes-bundler failed to start :(\n{:?}", e);
        }
    }
}

fn run(
    mut game_loop: GameLoop<Game, Time>,
    event_loop: winit::event_loop::EventLoop<()>,
    mut sdl_event_pump: EventPump,
) -> ! {
    event_loop.run(move |event, _, control_flow| {
        if log::max_level() == log::Level::Trace && Time::now().sub(&game_loop.last_stats) >= 1.0 {
            let (ups, rps, ..) = game_loop.get_stats();
            log::trace!("UPS: {:?}, RPS: {:?}", ups, rps);
        }

        let mut events: Vec<GuiEvent> = sdl_event_pump
            .poll_iter()
            .filter_map(|sdl_event| sdl_event.to_gamepad_event().map(GuiEvent::Gamepad))
            .collect();
        let game = &mut game_loop.game;

        match &event {
            winit::event::Event::WindowEvent { event, .. } => {
                use winit::event::WindowEvent;
                if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed) {
                    *control_flow = winit::event_loop::ControlFlow::Exit;
                }

                let gl_window = &mut game.gl_window;
                if let winit::event::WindowEvent::Resized(physical_size) = &event {
                    gl_window.resize(*physical_size);
                } else if let winit::event::WindowEvent::ScaleFactorChanged {
                    new_inner_size, ..
                } = &event
                {
                    gl_window.resize(**new_inner_size);
                }

                if !game.gui.on_event(event) {
                    if let Some(winit_gui_event) = &event.to_gui_event() {
                        events.push(winit_gui_event.clone());
                    }
                }
            }
            winit::event::Event::LoopDestroyed => {
                game.gui.destroy();
                return;
            }
            _ => {}
        }

        for event in events {
            let consumed = match &event {
                settings::gui::GuiEvent::Keyboard(KeyEvent::Pressed(key_code, modifiers)) => {
                    let settings = &mut game.settings;

                    use crate::input::keys::KeyCode::*;
                    match key_code {
                        F1 => {
                            settings.last_save_state = Some(b64.encode(game.state_handler.save()));
                            settings.save();
                            true
                        }
                        F2 => {
                            if let Some(save_state) = &settings.last_save_state {
                                if let Ok(buf) = &mut b64.decode(save_state) {
                                    game.state_handler.load(buf);
                                }
                            }
                            true
                        }
                        Escape => {
                            game.gui.toggle_visibility();
                            true
                        }

                        key_code => game
                            .gl_window
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
            |g| {
                let game = &mut g.game;

                #[allow(unused_mut)] //debug feature needs this
                let mut fps = game.advance();
                #[cfg(feature = "debug")]
                if game.debug.settings.override_fps {
                    fps = game.debug.settings.fps;
                }

                // No need to update graphics or audio more than once per update
                game.draw_frame();
                game.push_audio();

                g.set_updates_per_second(fps);
            },
            |g| {
                if let winit::event::Event::RedrawEventsCleared = &event {
                    let game = &mut g.game;
                    if game.run_gui() {
                        game.settings.save();
                    }

                    let gl_window = &game.gl_window;
                    unsafe {
                        use glow::HasContext as _;
                        //gl.clear_color(clear_colour[0], clear_colour[1], clear_colour[2], 1.0);
                        game.gl_window.glow_context.clear(glow::COLOR_BUFFER_BIT);
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
    ),
    anyhow::Error,
> {
    sdl2::hint::set("SDL_JOYSTICK_THREAD", "1");
    let sdl_context: Sdl = sdl2::init().map_err(anyhow::Error::msg)?;
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
    );

    let egui_glow = egui_glow::EguiGlow::new(&event_loop, gl_window.glow_context.clone(), None);
    egui_glow.egui_ctx.set_pixels_per_point(gl_window.get_dpi());
    let mut settings = Settings::new(bundle.config.default_settings.clone());
    let audio = Audio::new(&sdl_context, &settings)?;
    let nes = start_nes(bundle.rom.clone(), audio.stream.get_sample_rate() as u64)?;
    let state = LocalGameState::new(nes)?;
    let state_handler = LocalStateHandler {
        state,
        gui: EmptyGuiComponent::new(),
    };
    #[cfg(feature = "netplay")]
    let state_handler =
        netplay::NetplayStateHandler::new(state_handler, &bundle, &mut settings.netplay_id);
    let inputs = Inputs::new(&sdl_context, bundle.config.default_settings.input.selected);
    let sdl_event_pump = sdl_context.event_pump().map_err(anyhow::Error::msg)?;
    let game_loop: GameLoop<Game, Time> = GameLoop::new(
        Game::new(
            Box::new(state_handler),
            gl_window,
            Gui::new(true, egui_glow),
            settings,
            audio,
            inputs,
        ),
        FPS,
        0.08,
    );
    Ok((game_loop, event_loop, sdl_event_pump))
}

pub struct LocalGameState {
    nes: NesState,
    frame: i32,
}

impl LocalGameState {
    fn new(nes: NesState) -> Result<Self> {
        Ok(Self { nes, frame: 0 })
    }

    pub fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Fps {
        self.nes.p1_input = inputs[0].0;
        self.nes.p2_input = inputs[1].0;
        self.nes.run_until_vblank();
        self.frame += 1;
        FPS
    }

    fn save(&self) -> Vec<u8> {
        let mut data = self.nes.save_state();
        data.extend(self.frame.to_le_bytes());
        log::debug!("State saved at frame {:?}", self.frame);
        data
    }
    fn load(&mut self, data: &mut Vec<u8>) {
        self.frame = i32::from_le_bytes(
            data.split_off(data.len() - std::mem::size_of::<i32>())
                .try_into()
                .unwrap(),
        );
        self.nes.load_state(data);
        log::debug!("State loaded at frame {:?}", self.frame);
    }

    fn consume_samples(&mut self) -> Vec<i16> {
        self.nes.apu.consume_samples()
    }

    fn get_frame(&self) -> &Vec<u16> {
        &self.nes.ppu.screen
    }
}

impl Clone for LocalGameState {
    fn clone(&self) -> Self {
        let data = &mut self.save();
        let mut clone = Self {
            nes: NesState::new(self.nes.mapper.clone()),
            frame: 0,
        };
        clone.load(data);
        clone
    }
}

pub trait StateHandler {
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Fps;
    fn consume_samples(&mut self) -> Vec<i16>;
    fn get_frame(&self) -> Option<&Vec<u16>>;
    fn save(&self) -> Vec<u8>;
    fn load(&mut self, data: &mut Vec<u8>);
    fn get_gui(&mut self) -> &mut dyn GuiComponent;
}

pub struct LocalStateHandler {
    state: LocalGameState,
    gui: EmptyGuiComponent,
}

impl StateHandler for LocalStateHandler {
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Fps {
        self.state.advance(inputs)
    }
    fn consume_samples(&mut self) -> Vec<i16> {
        self.state.consume_samples()
    }
    fn get_frame(&self) -> Option<&Vec<u16>> {
        Some(self.state.get_frame())
    }
    fn save(&self) -> Vec<u8> {
        self.state.save()
    }
    fn load(&mut self, data: &mut Vec<u8>) {
        self.state.load(data)
    }

    fn get_gui(&mut self) -> &mut dyn GuiComponent {
        &mut self.gui
    }
}

struct Game {
    state_handler: Box<dyn StateHandler>,
    gl_window: GlutinWindowContext,
    gui: Gui,
    settings: Settings,
    #[cfg(feature = "debug")]
    debug: debug::Debug,
    audio: Audio,
    input: Input,
}
impl Game {
    pub fn new(
        state_handler: Box<dyn StateHandler>,
        gl_window: GlutinWindowContext,
        gui: Gui,
        settings: Settings,
        audio: Audio,
        inputs: Inputs,
    ) -> Self {
        Self {
            state_handler,
            gl_window,
            gui,
            input: Input::new(inputs),
            settings,
            #[cfg(feature = "debug")]
            debug: debug::Debug {
                settings: debug::DebugSettings::new(),
                gui: debug::gui::DebugGui::new(),
            },
            audio,
        }
    }
    fn apply_gui_event(&mut self, gui_event: GuiEvent) {
        self.gui.handle_events(
            &gui_event,
            vec![
                #[cfg(feature = "debug")]
                &mut self.debug,
                &mut self.audio,
                &mut self.input,
                self.state_handler.get_gui(),
            ],
            &mut self.settings,
        )
    }

    fn run_gui(&mut self) -> bool {
        let settings_hash_before = self.settings.get_hash();
        self.audio.sync_audio_devices(&mut self.settings.audio);

        self.gui.ui(
            self.gl_window.window(),
            vec![
                #[cfg(feature = "debug")]
                &mut self.debug,
                &mut self.audio,
                &mut self.input,
                self.state_handler.get_gui(),
            ],
            &mut self.settings,
        );
        settings_hash_before != self.settings.get_hash()
    }

    pub fn advance(&mut self) -> Fps {
        let input = &self.input;
        let inputs = [input.inputs.get_joypad(0), input.inputs.get_joypad(1)];

        self.state_handler.advance(inputs)
    }

    pub fn draw_frame(&mut self) {
        let new_image_data = self.state_handler.get_frame().map(|frame| {
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

    fn push_audio(&mut self) {
        self.audio
            .stream
            .push_samples(self.state_handler.consume_samples().as_slice());
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
