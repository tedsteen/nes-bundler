#![deny(clippy::all)]
#![forbid(unsafe_code)]

use std::cell::RefCell;
use std::fs::File;
use std::rc::Rc;

use crate::input::JoypadInput;
use anyhow::{Context, Result};
use audio::Audio;
use settings::gui::GuiComponent;

use crate::gameloop::game_loop;
use base64::engine::general_purpose::STANDARD_NO_PAD as b64;
use base64::Engine;

use gui::Framework;
use input::{Input, Inputs};
use palette::NTSC_PAL;
use pixels::{Pixels, SurfaceTexture};
use rfd::FileDialog;
use rusticnes_core::cartridge::mapper_from_file;
use rusticnes_core::nes::NesState;
use sdl2::Sdl;
use serde::Deserialize;
use settings::{Settings, MAX_PLAYERS};
use tinyfiledialogs::MessageBoxIcon;
use winit::dpi::LogicalSize;
use winit::event::{Event, VirtualKeyCode, WindowEvent};
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

mod audio;
#[cfg(feature = "debug")]
mod debug;
mod gameloop;
mod gui;
mod input;
#[cfg(feature = "netplay")]
mod netplay;
mod palette;
mod settings;

type Fps = u32;
const FPS: Fps = 60;
const WIDTH: u32 = 256;
const HEIGHT: u32 = 240;
const ZOOM: f32 = 3.0;

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

fn get_static_bundle() -> Result<Option<Bundle>> {
    #[cfg(feature = "static-bundle")]
    return Ok(Some(Bundle {
        config: serde_yaml::from_str(include_str!("../config/config.yaml"))?,
        rom: include_bytes!("../config/rom.nes").to_vec(),
    }));
    #[cfg(not(feature = "static-bundle"))]
    return Ok(None);
}

fn load_bundle() -> Result<Bundle> {
    if let Some(bundle) = get_static_bundle()? {
        Ok(bundle)
    } else if let Ok(zip_file) = File::open("bundle.zip") {
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
        let folder = FileDialog::new()
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
        load_bundle()
    }
}
#[derive(Deserialize, Debug)]
pub struct BuildConfiguration {
    window_title: String,
    default_settings: Settings,
    #[cfg(feature = "netplay")]
    netplay: netplay::NetplayBuildConfiguration,
}
fn main() {
    match load_bundle() {
        Ok(bundle) => {
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
            run(bundle);
        }
        Err(e) => {
            tinyfiledialogs::message_box_ok(
                "Could not load the bundle",
                &format!("{:?}", e).replace('\'', "´").replace('\"', "``"),
                MessageBoxIcon::Error,
            );
        }
    }
}

fn run(bundle: Bundle) -> ! {
    let window_title = bundle.config.window_title.clone();
    match initialise(bundle) {
        Ok((event_loop, window, framework, game_runner)) => {
            game_loop(
                event_loop,
                window,
                (game_runner, framework),
                FPS,
                0.08,
                move |g| {
                    let game_runner = &mut g.game.0;
                    let fps = game_runner.advance();
                    g.set_updates_per_second(fps);
                },
                move |g| {
                    let game_runner = &mut g.game.0;
                    game_runner.render(&g.window, &mut g.game.1);
                },
                move |g, event| {
                    let game_runner = &mut g.game.0;
                    if !game_runner.handle(event, &mut g.game.1) {
                        g.exit();
                    }
                },
            );
        }
        Err(e) => {
            tinyfiledialogs::message_box_ok(
                &format!("Could not start {}", window_title),
                &format!("{:?}", e).replace('\'', "´").replace('\"', "``"),
                MessageBoxIcon::Error,
            );
            std::process::exit(0)
        }
    }
}

fn initialise(
    bundle: Bundle,
) -> Result<(EventLoop<()>, winit::window::Window, Framework, GameRunner)> {
    // This is required for certain controllers to work on Windows without the
    // video subsystem enabled:
    sdl2::hint::set("SDL_JOYSTICK_THREAD", "1");
    let sdl_context: Sdl = sdl2::init().map_err(anyhow::Error::msg)?;
    let event_loop = EventLoop::new();
    let window = {
        WindowBuilder::new()
            .with_title(&bundle.config.window_title)
            .with_inner_size(LogicalSize::new(WIDTH as f32 * ZOOM, HEIGHT as f32 * ZOOM))
            .with_min_inner_size(LogicalSize::new(WIDTH, HEIGHT))
            .build(&event_loop)?
    };
    let (pixels, framework) = {
        let window_size = window.inner_size();
        let scale_factor = window.scale_factor() as f32;
        let surface_texture = SurfaceTexture::new(window_size.width, window_size.height, &window);
        let pixels = Pixels::new(WIDTH, HEIGHT, surface_texture).context("No pixels available")?;

        let framework = Framework::new(
            &event_loop,
            window_size.width,
            window_size.height,
            scale_factor,
            &pixels,
        );

        (pixels, framework)
    };

    let settings = Rc::new(RefCell::new(Settings::new(&bundle.config.default_settings)));

    let audio = Audio::new(&sdl_context, settings.clone())?;
    let nes = start_nes(bundle.rom.clone(), audio.stream.get_sample_rate() as u64)?;
    let state = LocalGameState::new(nes)?;

    #[cfg(feature = "netplay")]
    let state_handler = netplay::state_handler::NetplayStateHandler::new(
        state,
        &bundle,
        &mut settings.borrow_mut().netplay_id,
    );

    #[cfg(not(feature = "netplay"))]
    let state_handler = LocalStateHandler {
        state,
        gui: EmptyGuiComponent { is_open: false },
    };

    let inputs = Inputs::new(
        &sdl_context,
        bundle.config.default_settings.input.selected,
        settings.clone(),
    );
    let input = Input::new(inputs, settings.clone());

    let game_runner = GameRunner::new(pixels, audio, input, settings, Box::new(state_handler))?;
    Ok((event_loop, window, framework, game_runner))
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
        //println!("Advancing! {:?}", inputs);
        self.nes.p1_input = inputs[0].0;
        self.nes.p2_input = inputs[1].0;
        self.nes.run_until_vblank();
        self.frame += 1;
        FPS
    }

    fn save(&self) -> Vec<u8> {
        let mut data = self.nes.save_state();
        data.extend(self.frame.to_le_bytes());
        //println!("SAVED {:?}", self.frame);
        data
    }
    fn load(&mut self, data: &mut Vec<u8>) {
        self.frame = i32::from_le_bytes(
            data.split_off(data.len() - std::mem::size_of::<i32>())
                .try_into()
                .unwrap(),
        );
        self.nes.load_state(data);
        //println!("LOADED {:?}", self.frame);
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
    fn get_frame(&self) -> &Vec<u16>;
    fn save(&self) -> Vec<u8>;
    fn load(&mut self, data: &mut Vec<u8>);
    fn get_gui(&mut self) -> &mut dyn GuiComponent;
}

struct LocalStateHandler {
    state: LocalGameState,
    gui: EmptyGuiComponent,
}

struct EmptyGuiComponent {
    is_open: bool,
}

impl GuiComponent for EmptyGuiComponent {
    fn ui(&mut self, _ctx: &egui::Context, _ui_visible: bool, _name: String) {}
    fn name(&self) -> Option<String> {
        None
    }
    fn open(&mut self) -> &mut bool {
        &mut self.is_open
    }

    fn event(&mut self, _event: &winit::event::Event<()>) {}
}

impl StateHandler for LocalStateHandler {
    fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Fps {
        self.state.advance(inputs)
    }
    fn consume_samples(&mut self) -> Vec<i16> {
        self.state.consume_samples()
    }
    fn get_frame(&self) -> &Vec<u16> {
        self.state.get_frame()
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
    state_handler: Box<dyn StateHandler>,
    audio: Audio,
    pixels: Pixels,
    settings: Rc<RefCell<Settings>>,
    input: Input,
    #[cfg(feature = "debug")]
    debug: debug::Debug,
}

impl GameRunner {
    pub fn new(
        pixels: Pixels,
        audio: Audio,
        input: Input,
        settings: Rc<RefCell<Settings>>,
        state_handler: Box<dyn StateHandler>,
    ) -> Result<Self> {
        Ok(Self {
            state_handler,
            audio,
            pixels,
            input,
            settings,
            #[cfg(feature = "debug")]
            debug: debug::Debug {
                settings: debug::DebugSettings::new(),
                gui: debug::gui::DebugGui::new(),
            },
        })
    }
    pub fn advance(&mut self) -> Fps {
        let inputs = [
            self.input.inputs.get_joypad(0),
            self.input.inputs.get_joypad(1),
        ];

        let fps = self.state_handler.advance(inputs);

        self.audio
            .stream
            .push_samples(self.state_handler.consume_samples().as_slice());

        #[cfg(feature = "debug")]
        if self.debug.settings.override_fps {
            return self.debug.settings.fps;
        }
        fps
    }

    pub fn render(&mut self, window: &winit::window::Window, gui_framework: &mut Framework) {
        let settings_hash_before = self.settings.borrow().get_hash();

        gui_framework.prepare(
            window,
            &mut vec![
                #[cfg(feature = "debug")]
                &mut self.debug,
                &mut self.audio,
                &mut self.input,
                self.state_handler.get_gui(),
            ],
        );

        if settings_hash_before != self.settings.borrow().get_hash() {
            self.settings.borrow().save().unwrap();
        }

        let pixels = &mut self.pixels;

        let frame = self.state_handler.get_frame();

        for (i, pixel) in pixels.frame_mut().chunks_exact_mut(4).enumerate() {
            let palette_index = frame[i] as usize * 4;
            pixel.copy_from_slice(&NTSC_PAL[palette_index..palette_index + 4]);
        }

        // Render everything together
        pixels
            .render_with(|encoder, render_target, context| {
                // Render the world texture
                context.scaling_renderer.render(encoder, render_target);

                // Render egui
                gui_framework.render(encoder, render_target, context);

                Ok(())
            })
            .expect("Failed to render :(");
    }

    pub fn handle(
        &mut self,
        event: &winit::event::Event<()>,
        gui_framework: &mut Framework,
    ) -> bool {
        // Handle input events
        if let Event::WindowEvent { event, .. } = event {
            match event {
                WindowEvent::CloseRequested => {
                    return false;
                }
                WindowEvent::Resized(size) => {
                    self.pixels.resize_surface(size.width, size.height).unwrap();
                }
                WindowEvent::KeyboardInput { input, .. } => {
                    if input.state == winit::event::ElementState::Pressed {
                        match input.virtual_keycode {
                            Some(VirtualKeyCode::F1) => {
                                self.save_state();
                            }
                            Some(VirtualKeyCode::F2) => {
                                self.load_state();
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        // Update egui inputs
        gui_framework.handle_event(
            event,
            vec![
                #[cfg(feature = "debug")]
                &mut self.debug,
                &mut self.audio,
                &mut self.input,
                self.state_handler.get_gui(),
            ],
        );

        true
    }

    fn save_state(&mut self) {
        self.settings.borrow_mut().last_save_state = Some(b64.encode(self.state_handler.save()));
    }

    fn load_state(&mut self) {
        if let Some(save_state) = &mut self.settings.borrow_mut().last_save_state {
            if let Ok(buf) = &mut b64.decode(save_state) {
                self.state_handler.load(buf);
                self.audio.stream.drain(); //make sure we don't build up a delay
            }
        }
    }
}
