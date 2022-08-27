#![deny(clippy::all)]
#![forbid(unsafe_code)]

use std::collections::hash_map::DefaultHasher;
use std::env;
use std::fs::File;
use std::hash::Hash;
use std::io::Read;

use crate::input::JoypadInput;
use anyhow::Result;
use audio::{Audio, Stream};

use cpal::traits::{DeviceTrait, HostTrait};
use cpal::Device;
use game_loop::game_loop;

use gui::Framework;
use input::Inputs;
use palette::NTSC_PAL;
use pixels::{Pixels, SurfaceTexture};
use rusticnes_core::cartridge::mapper_from_file;
use rusticnes_core::nes::NesState;
use serde::Deserialize;
use settings::{Settings, MAX_PLAYERS};
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
    match mapper_from_file(cart_data.as_slice()) {
        Ok(mapper) => {
            mapper.print_debug_status();
            let mut nes = NesState::new(mapper);
            nes.power_on();
            Ok(nes)
        }
        _err => Err("ouch".to_owned()),
    }
}
struct Bundle {
    config: BuildConfiguration,
    rom: Vec<u8>,
}

fn extract_bundle() -> Result<Bundle> {
    let mut zip = zip::ZipArchive::new(File::open(env::current_exe()?)?)?;
    let config: BuildConfiguration = serde_yaml::from_reader(zip.by_name("config.yaml")?)?;
    let mut rom = Vec::new();
    zip.by_name("rom.nes")?.read_to_end(&mut rom)?;
    Ok(Bundle { config, rom })
}

#[derive(Deserialize, Debug)]
pub struct BuildConfiguration {
    window_title: String,
    default_settings: Settings,
    #[cfg(feature = "netplay")]
    netplay: netplay::NetplayBuildConfiguration,
}
fn main() -> Result<()> {
    let bundle = extract_bundle()
        .map_err(|err| anyhow::Error::msg(format!("Could not extract bundle config ({err})")))?;

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

    let event_loop = EventLoop::new();

    let window = {
        WindowBuilder::new()
            .with_title(&bundle.config.window_title)
            .with_inner_size(LogicalSize::new(WIDTH as f32 * ZOOM, HEIGHT as f32 * ZOOM))
            .with_min_inner_size(LogicalSize::new(WIDTH, HEIGHT))
            .build(&event_loop)
            .unwrap()
    };

    let (pixels, framework) = {
        let window_size = window.inner_size();
        let scale_factor = window.scale_factor() as f32;
        let surface_texture = SurfaceTexture::new(window_size.width, window_size.height, &window);
        let pixels = Pixels::new(WIDTH, HEIGHT, surface_texture).expect("No pixels available");
        let framework =
            Framework::new(window_size.width, window_size.height, scale_factor, &pixels);

        (pixels, framework)
    };

    let game_runner = GameRunner::new(pixels, &bundle.config, bundle.rom);
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
                let device_changed = true;
                if device_changed {
                    game_runner
                        .sound_stream
                        .set_output_device(GameRunner::get_output_device(&game_runner.settings))
                }
                if game_runner.sound_stream.get_latency() != game_runner.settings.audio.latency {
                    game_runner
                        .sound_stream
                        .set_latency(game_runner.settings.audio.latency);
                }
                game_runner
                    .sound_stream
                    .set_volume(game_runner.settings.audio.volume);

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

pub struct MyGameState {
    nes: NesState,
}

impl MyGameState {
    fn new(rom: Vec<u8>) -> Self {
        let rom_data = match std::env::var("ROM_FILE") {
            Ok(rom_file) => std::fs::read(&rom_file)
                .unwrap_or_else(|_| panic!("Could not read ROM {}", rom_file)),
            Err(_e) => rom.to_vec(),
        };

        let nes = load_rom(rom_data).expect("Failed to load ROM");

        Self { nes }
    }

    pub fn advance(&mut self, inputs: [JoypadInput; MAX_PLAYERS]) -> Fps {
        //println!("Advancing! {:?}", inputs);
        self.nes.p1_input = inputs[0].0;
        self.nes.p2_input = inputs[1].0;
        self.nes.run_until_vblank();
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
        self.nes.save_state()
    }
    fn load(&mut self, data: &mut Vec<u8>) {
        self.nes.load_state(data);
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
    pub fn get_output_device(settings: &Settings) -> Device {
        settings
            .audio
            .output_device
            .clone()
            .and_then(|device_name| {
                let host = cpal::default_host();
                if let Ok(mut output_devices) = host.output_devices() {
                    output_devices
                        .find(|output_device| output_device.name().unwrap() == device_name)
                } else {
                    None
                }
            })
            .or_else(|| cpal::default_host().default_output_device())
            .expect("No audio output device found :(")
    }

    pub fn new(pixels: Pixels, build_config: &BuildConfiguration, rom: Vec<u8>) -> Self {
        #[allow(unused_mut)] // needs to be mut for netplay feature
        let mut settings: Settings = Settings::new(&build_config.default_settings);

        let inputs = Inputs::new(
            build_config.default_settings.input.selected.clone(),
            &mut settings.input,
        );

        let mut game_hash = DefaultHasher::new();
        rom.hash(&mut game_hash);

        let audio = Audio::new();
        let output_device = GameRunner::get_output_device(&settings);
        println!("Output device : {}", output_device.name().unwrap());

        let sound_stream = audio
            .start(output_device, &settings.audio)
            .expect("Could not start Audio");
        let mut state = MyGameState::new(rom);
        state
            .nes
            .apu
            .set_sample_rate(sound_stream.get_sample_rate() as u64);

        Self {
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
        }
    }
    pub fn advance(&mut self) -> Fps {
        #[cfg(not(feature = "netplay"))]
        let fps = self
            .state
            .advance([self.inputs.get_joypad(0), self.inputs.get_joypad(1)]);

        #[cfg(feature = "netplay")]
        let fps = self.netplay.advance(
            &mut self.state,
            [self.inputs.get_joypad(0), self.inputs.get_joypad(1)],
        );

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

        let frame = pixels.get_frame();
        self.state.render(frame);

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
                    self.pixels.resize_surface(size.width, size.height);
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
