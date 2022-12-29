#![deny(clippy::all)]
#![forbid(unsafe_code)]

use std::collections::hash_map::DefaultHasher;
use std::fs::File;
use std::hash::Hash;

use crate::input::JoypadInput;
use anyhow::{Context, Result};
use audio::Stream;

use game_loop::game_loop;

use gui::Framework;
use input::Inputs;
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
use winit::event::{Event, VirtualKeyCode};
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

mod audio;
#[cfg(feature = "debug")]
mod debug;
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

pub fn load_rom(cart_data: Vec<u8>) -> Result<NesState, String> {
    let mapper = mapper_from_file(cart_data.as_slice())?;
    mapper.print_debug_status();
    let mut nes = NesState::new(mapper);
    nes.power_on();
    Ok(nes)
}
struct Bundle {
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
                &format!("{:?}", e).replace("'", "´").replace("\"", "``"),
                MessageBoxIcon::Error,
            );
        }
    }
}

fn run(bundle: Bundle) -> ! {
    let window_title = bundle.config.window_title.clone();
    match initialise(bundle) {
        Ok((event_loop, window, framework, game_runner)) => {
            let mut last_settings = game_runner.settings.get_hash();
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
                    let curr_settings = game_runner.settings.get_hash();
                    if last_settings != curr_settings {
                        if *game_runner.sound_stream.get_output_device_name()
                            != game_runner.settings.audio.output_device
                        {
                            game_runner
                                .sound_stream
                                .set_output_device(game_runner.settings.audio.output_device.clone())
                        }

                        // Note: We might not get the exact latency in ms since there will be rounding errors. Be ok with 1-off
                        if i16::abs(
                            game_runner.sound_stream.get_latency() as i16
                                - game_runner.settings.audio.latency as i16,
                        ) > 1
                        {
                            game_runner
                                .sound_stream
                                .set_latency(game_runner.settings.audio.latency);

                            //Whatever latency we ended up getting, save that to settings
                            game_runner.settings.audio.latency =
                                game_runner.sound_stream.get_latency();
                        }
                        game_runner.sound_stream.volume =
                            game_runner.settings.audio.volume as f32 / 100.0;

                        last_settings = curr_settings;
                        if game_runner.settings.save().is_err() {
                            eprintln!("Failed to save the settings");
                        }
                    }
                    if !game_runner.handle(event, &mut g.game.1) {
                        g.exit();
                    }
                },
            );
        }
        Err(e) => {
            tinyfiledialogs::message_box_ok(
                &format!("Could not start {}", window_title),
                &format!("{:?}", e).replace("'", "´").replace("\"", "``"),
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
    #[allow(unused_mut)] // needs to be mut for netplay feature
    let mut settings: Settings = Settings::new(&bundle.config.default_settings);
    let game_runner = GameRunner::new(pixels, sdl_context, &bundle.config, settings, bundle.rom)?;
    Ok((event_loop, window, framework, game_runner))
}
pub struct MyGameState {
    nes: NesState,
    frame: i32,
}

impl MyGameState {
    fn new(rom: Vec<u8>) -> Result<Self> {
        let rom_data = match std::env::var("ROM_FILE") {
            Ok(rom_file) => {
                std::fs::read(&rom_file).context(format!("Could not read ROM {}", rom_file))?
            }
            Err(_e) => rom.to_vec(),
        };

        let nes = load_rom(rom_data)
            .map_err(anyhow::Error::msg)
            .context("Failed to load ROM")?;

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

    fn render(&self, frame: &mut [u8]) {
        let screen = &self.nes.ppu.screen;

        for (i, pixel) in frame.chunks_exact_mut(4).enumerate() {
            let palette_index = screen[i] as usize * 4;
            pixel.copy_from_slice(&NTSC_PAL[palette_index..palette_index + 4]);
        }
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
    fn reset(&mut self) {
        self.nes.reset();
        self.frame = 0;
    }
}

pub struct GameRunner {
    state: MyGameState,
    sound_stream: Stream,
    pixels: Pixels,
    settings: Settings,
    inputs: Inputs,

    #[cfg(feature = "netplay")]
    netplay: netplay::Netplay,
    #[cfg(feature = "debug")]
    debug: debug::DebugSettings,
}

impl GameRunner {
    pub fn new(
        pixels: Pixels,
        sdl_context: Sdl,
        build_config: &BuildConfiguration,
        mut settings: Settings,
        rom: Vec<u8>,
    ) -> Result<Self> {
        let inputs = Inputs::new(
            &sdl_context,
            build_config.default_settings.input.selected.clone(),
            &mut settings.input,
        );

        let mut game_hash = DefaultHasher::new();
        rom.hash(&mut game_hash);

        let audio_subsystem = sdl_context.audio().map_err(anyhow::Error::msg)?;

        let sound_stream = Stream::new(&audio_subsystem, &settings.audio);
        let mut state = MyGameState::new(rom)?;
        state
            .nes
            .apu
            .set_sample_rate(sound_stream.get_sample_rate() as u64);

        Ok(Self {
            state,
            sound_stream,
            pixels,
            #[cfg(feature = "netplay")]
            netplay: netplay::Netplay::new(
                &build_config.netplay,
                &mut settings,
                std::hash::Hasher::finish(&game_hash),
            ),
            settings,
            inputs,
            #[cfg(feature = "debug")]
            debug: debug::DebugSettings::new(),
        })
    }
    pub fn advance(&mut self) -> Fps {
        let inputs = [self.inputs.get_joypad(0), self.inputs.get_joypad(1)];

        #[cfg(not(feature = "netplay"))]
        let fps = self.state.advance(inputs);

        #[cfg(feature = "netplay")]
        let fps = self.netplay.advance(&mut self.state, inputs);

        self.sound_stream
            .push_samples(self.state.nes.apu.consume_samples().as_slice());

        #[cfg(feature = "debug")]
        if self.debug.override_fps {
            return self.debug.fps;
        }
        fps
    }

    pub fn render(&mut self, window: &winit::window::Window, gui_framework: &mut Framework) {
        gui_framework.prepare(window, self);

        let pixels = &mut self.pixels;

        self.state.render(pixels.get_frame_mut());

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
        self.inputs.advance(event, &mut self.settings.input);
        // Handle input events
        if let Event::WindowEvent { event, .. } = event {
            match event {
                winit::event::WindowEvent::CloseRequested => {
                    return false;
                }
                winit::event::WindowEvent::Resized(size) => {
                    self.pixels.resize_surface(size.width, size.height).unwrap();
                }
                winit::event::WindowEvent::KeyboardInput { input, .. } => {
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
            // Update egui inputs
            gui_framework.handle_event(event, self);
        }
        true
    }

    fn save_state(&mut self) {
        self.settings.last_save_state = Some(base64::encode(self.state.save()));
    }

    fn load_state(&mut self) {
        if let Some(save_state) = &mut self.settings.last_save_state {
            if let Ok(buf) = &mut base64::decode(save_state) {
                self.state.load(buf);
                self.sound_stream.drain(); //make sure we don't build up a delay
            }
        }
    }
}
