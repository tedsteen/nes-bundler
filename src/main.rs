#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(unsafe_code)]
#![deny(clippy::all)]

use std::cell::RefCell;
use std::rc::Rc;

use crate::settings::gui::ToGuiEvent;
use crate::window::{create_display, Fullscreen, GlutinWindowContext};
use crate::{
    gameloop::TimeTrait,
    input::{gamepad::ToGamepadEvent, JoypadInput, KeyEvent},
    settings::gui::{Gui, GuiEvent},
};
use anyhow::{Context, Result};
use audio::Audio;
use egui::{epaint::ImageDelta, Color32, ColorImage, ImageData, TextureOptions};
use gameloop::{GameLoop, Time};
use settings::gui::{EmptyGuiComponent, GuiComponent};

use base64::engine::general_purpose::STANDARD_NO_PAD as b64;
use base64::Engine;

use input::{Input, Inputs};
use palette::NTSC_PAL;
use rusticnes_core::cartridge::mapper_from_file;
use rusticnes_core::nes::NesState;
use sdl2::Sdl;
use serde::Deserialize;
use settings::{Settings, MAX_PLAYERS};

mod audio;
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

pub struct Bundle {
    config: BuildConfiguration,
    rom: Vec<u8>,
}

#[cfg(feature = "zip-bundle")]
fn load_bundle_from_zip(zip_file: std::io::Result<std::fs::File>) -> Result<Bundle> {
    if let Ok(zip_file) = zip_file {
        let mut zip = zip::ZipArchive::new(zip_file)?;
        let config: BuildConfiguration = serde_yaml::from_reader(
            zip.by_name("config.yaml")
                .context("config.yaml not found in bundle.zip")?,
        )?;

        let mut rom = Vec::new();
        std::io::copy(
            &mut zip
                .by_name("rom.nes")
                .context("rom.nes not found in bundle.zip")?,
            &mut rom,
        )?;
        Ok(Bundle { config, rom })
    } else {
        let folder = rfd::FileDialog::new()
            .set_title("Files to bundle")
            .set_directory(".")
            .pick_folder()
            .context("No bundle to load")?;

        let mut config_path = folder.clone();
        config_path.push("config.yaml");
        let mut config_file = std::fs::File::open(config_path)
            .context(format!("config.yaml not found in {:?}", folder))?;

        let mut rom_path = folder.clone();
        rom_path.push("rom.nes");
        let mut rom_file =
            std::fs::File::open(rom_path).context(format!("rom.nes not found in {:?}", folder))?;

        let mut zip = zip::ZipWriter::new(
            std::fs::File::create("bundle.zip").context("Could not create bundle.zip")?,
        );
        zip.start_file("config.yaml", Default::default())?;
        std::io::copy(&mut config_file, &mut zip)?;

        zip.start_file("rom.nes", Default::default())?;
        std::io::copy(&mut rom_file, &mut zip)?;

        zip.finish()?;

        // Try again with newly created bundle.zip
        load_bundle_from_zip(std::fs::File::open("bundle.zip"))
    }
}
fn load_bundle() -> Result<Bundle> {
    #[cfg(not(feature = "zip-bundle"))]
    return Ok(Bundle {
        config: serde_yaml::from_str(include_str!("../config/config.yaml"))?,
        rom: include_bytes!("../config/rom.nes").to_vec(),
    });
    #[cfg(feature = "zip-bundle")]
    {
        let res = load_bundle_from_zip(std::fs::File::open("bundle.zip"));
        if let Err(e) = &res {
            log::error!("Could not load the bundle: {:?}", e);
        }
        res
    }
}
#[derive(Deserialize, Debug)]
pub struct BuildConfiguration {
    window_title: String,
    default_settings: Settings,
    #[cfg(feature = "netplay")]
    netplay: netplay::NetplayBuildConfiguration,
}

fn main() -> Result<()> {
    env_logger::init();

    // This is required for certain controllers to work on Windows without the
    // video subsystem enabled:
    sdl2::hint::set("SDL_JOYSTICK_THREAD", "1");
    let sdl_context: Sdl = sdl2::init().map_err(anyhow::Error::msg)?;

    let bundle = load_bundle()?;
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
    let (gl_window, gl) = create_display(
        &bundle.config.window_title,
        DEFAULT_WINDOW_SIZE.0,
        DEFAULT_WINDOW_SIZE.1,
        &event_loop,
    );
    let gl = std::sync::Arc::new(gl);

    let egui_glow = egui_glow::EguiGlow::new(&event_loop, gl.clone(), None);
    egui_glow.egui_ctx.set_pixels_per_point(gl_window.get_dpi());

    let settings = Rc::new(RefCell::new(Settings::new(
        bundle.config.default_settings.clone(),
    )));

    let audio = Audio::new(&sdl_context, settings.clone())?;
    let nes = start_nes(bundle.rom.clone(), audio.stream.get_sample_rate() as u64)?;
    let state = LocalGameState::new(nes)?;

    let state_handler = LocalStateHandler {
        state,
        gui: EmptyGuiComponent::new(),
    };

    #[cfg(feature = "netplay")]
    let state_handler = netplay::NetplayStateHandler::new(
        state_handler,
        &bundle,
        &mut settings.borrow_mut().netplay_id,
    );

    let inputs = Inputs::new(&sdl_context, bundle.config.default_settings.input.selected);

    let nes_texture_options = TextureOptions {
        magnification: egui::TextureFilter::Nearest,
        minification: egui::TextureFilter::Nearest,
    };

    let no_image = ImageData::Color(ColorImage::new([0, 0], Color32::TRANSPARENT));

    let nes_texture = egui_glow.egui_ctx.load_texture(
        "nes",
        ImageData::Color(ColorImage::new(
            [WIDTH as usize, HEIGHT as usize],
            Color32::BLACK,
        )),
        nes_texture_options,
    );
    struct GameLoopState {
        game_runner: GameRunner,
        gl_window: GlutinWindowContext,
        egui_glow: egui_glow::EguiGlow,
        gui: Gui,
        settings: Rc<RefCell<Settings>>,
        #[cfg(feature = "debug")]
        debug: debug::Debug,
        audio: Audio,
        input: Input,
    }

    let mut game_loop: GameLoop<GameLoopState, Time> = GameLoop::new(
        GameLoopState {
            game_runner: GameRunner::new(Box::new(state_handler))?,
            gl_window,
            egui_glow,
            gui: Gui::new(true),
            settings: settings.clone(),
            #[cfg(feature = "debug")]
            debug: debug::Debug {
                settings: debug::DebugSettings::new(),
                gui: debug::gui::DebugGui::new(),
            },
            audio,
            input: Input::new(inputs, settings),
        },
        FPS,
        0.08,
    );
    let mut sdl_event_pump = sdl_context.event_pump().unwrap();

    event_loop.run(move |event, _, control_flow| {
        if log::max_level() == log::Level::Trace && Time::now().sub(&game_loop.last_stats) >= 1.0 {
            let (ups, rps, ..) = game_loop.get_stats();
            log::trace!("UPS: {:?}, RPS: {:?}", ups, rps);
        }
        let loop_state = &mut game_loop.game;

        let winit_gui_event = if let winit::event::Event::WindowEvent { event, .. } = &event {
            use winit::event::WindowEvent;
            if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed) {
                *control_flow = winit::event_loop::ControlFlow::Exit;
            }

            let gl_window = &mut loop_state.gl_window;
            if let winit::event::WindowEvent::Resized(physical_size) = &event {
                gl_window.resize(*physical_size);
            } else if let winit::event::WindowEvent::ScaleFactorChanged { new_inner_size, .. } =
                &event
            {
                gl_window.resize(**new_inner_size);
            }

            let winit_gui_event = event.to_gui_event();
            let egui_glow = &mut loop_state.egui_glow;
            if !egui_glow.on_event(event).consumed {
                if let Some(settings::gui::GuiEvent::Keyboard(KeyEvent::Pressed(
                    key_code,
                    modifiers,
                ))) = winit_gui_event.clone()
                {
                    let settings = &loop_state.settings;
                    let game_runner = &mut loop_state.game_runner;

                    use crate::input::keys::KeyCode::*;
                    match key_code {
                        F1 => {
                            let mut settings = settings.borrow_mut();
                            settings.last_save_state =
                                Some(b64.encode(game_runner.state_handler.save()));
                            settings.save().unwrap();
                        }
                        F2 => {
                            if let Some(save_state) = &settings.borrow().last_save_state {
                                if let Ok(buf) = &mut b64.decode(save_state) {
                                    game_runner.state_handler.load(buf);
                                }
                            }
                        }
                        key_code => {
                            gl_window
                                .window_mut()
                                .check_and_set_fullscreen(modifiers, key_code);
                        }
                    }
                }
            }
            winit_gui_event
        } else {
            None
        };

        let sdl2_gui_event = sdl_event_pump
            .poll_event()
            .and_then(|sdl_event| sdl_event.to_gamepad_event().map(GuiEvent::Gamepad));

        loop_state.gui.handle_events(
            [sdl2_gui_event, winit_gui_event].iter().flatten().collect(),
            vec![
                #[cfg(feature = "debug")]
                &mut loop_state.debug,
                &mut loop_state.audio,
                &mut loop_state.input,
                loop_state.game_runner.state_handler.get_gui(),
            ],
        );

        if let winit::event::Event::LoopDestroyed = &event {
            println!("DRESTORY");
            loop_state.egui_glow.destroy();
            return;
        }

        game_loop.next_frame(
            |g| {
                let loop_state = &mut g.game;
                let egui_glow = &mut loop_state.egui_glow;
                let game_runner = &mut loop_state.game_runner;

                #[allow(unused_mut)] //debug feature needs this
                let mut fps = game_runner.advance(&loop_state.input);
                #[cfg(feature = "debug")]
                if loop_state.debug.settings.override_fps {
                    fps = loop_state.debug.settings.fps;
                }

                // No need to update graphics or audio more than once per update
                let new_frame = game_runner.get_frame().unwrap_or_else(|| no_image.clone());
                egui_glow.egui_ctx.tex_manager().write().set(
                    nes_texture.id(),
                    ImageDelta::full(new_frame, nes_texture_options),
                );

                loop_state
                    .audio
                    .stream
                    .push_samples(game_runner.state_handler.consume_samples().as_slice());

                g.set_updates_per_second(fps);
            },
            |g| {
                if let winit::event::Event::RedrawEventsCleared = &event {
                    let loop_state = &mut g.game;
                    let gl_window = &loop_state.gl_window;
                    let settings = &loop_state.settings;
                    let settings_hash_before = settings.borrow().get_hash();
                    let gui = &mut loop_state.gui;

                    let egui_glow = &mut loop_state.egui_glow;

                    let window = &mut gl_window.window();

                    egui_glow.run(gl_window.window(), |egui_ctx| {
                        let game_runner = &mut loop_state.game_runner;
                        gui.ui(
                            egui_ctx,
                            &mut vec![
                                #[cfg(feature = "debug")]
                                &mut loop_state.debug,
                                &mut loop_state.audio,
                                &mut loop_state.input,
                                game_runner.state_handler.get_gui(),
                            ],
                            &nes_texture,
                        );
                    });

                    if settings_hash_before != settings.borrow().get_hash() {
                        log::debug!("Settings saved");
                        settings.borrow().save().unwrap();
                    }

                    unsafe {
                        use glow::HasContext as _;
                        //gl.clear_color(clear_colour[0], clear_colour[1], clear_colour[2], 1.0);
                        gl.clear(glow::COLOR_BUFFER_BIT);
                    }

                    // draw things behind egui here

                    egui_glow.paint(window);

                    // draw things on top of egui here

                    gl_window.swap_buffers().unwrap();
                }
            },
        );
    });
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

pub struct GameRunner {
    pub state_handler: Box<dyn StateHandler>,
}

impl GameRunner {
    pub fn new(state_handler: Box<dyn StateHandler>) -> Result<Self> {
        Ok(Self { state_handler })
    }
    pub fn advance(&mut self, input: &Input) -> Fps {
        let inputs = [input.inputs.get_joypad(0), input.inputs.get_joypad(1)];

        self.state_handler.advance(inputs)
    }

    pub fn get_frame(&mut self) -> Option<ImageData> {
        if let Some(frame) = self.state_handler.get_frame() {
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
            Some(image_data)
        } else {
            None
        }
    }
}
