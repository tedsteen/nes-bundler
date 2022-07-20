#![deny(clippy::all)]
#![forbid(unsafe_code)]

use std::fs::File;
use std::io::{Read, Write};

use crate::input::JoypadInput;
use anyhow::Result;
use audio::{Audio, Stream};

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
mod gui;
mod input;
#[cfg(feature = "netplay")]
mod network;
mod palette;
mod settings;

type Fps = u32;
const FPS: Fps = 60;
const WIDTH: u32 = 256;
const HEIGHT: u32 = 240;
const ZOOM: f32 = 2.0;

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

#[tokio::main]
async fn main() {
    async_main().await;
}
#[derive(Deserialize)]
struct BuildConfiguration {
    window_title: String,
    default_settings: Settings,
}
async fn async_main() {
    env_logger::init();
    let build_configuration: BuildConfiguration =
        serde_json::from_str(include_str!("../assets/build_config.json")).unwrap();

    let event_loop = EventLoop::new();

    let window = {
        WindowBuilder::new()
            .with_title(build_configuration.window_title)
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

    let game_runner = GameRunner::new(pixels, &build_configuration.default_settings);
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
                if game_runner.sound_stream.get_latency() != game_runner.settings.audio.latency {
                    game_runner
                        .sound_stream
                        .set_latency(game_runner.settings.audio.latency);
                }

                last_settings = curr_settings;
                if let anyhow::private::Err(err) = game_runner.settings.save() {
                    eprintln!("Failed to save the settings: {}", err);
                }
            }
            if !game_runner.handle(event, &mut g.game.1) {
                g.exit();
            }
        },
    );
}

pub(crate) struct MyGameState {
    nes: NesState,
}

impl MyGameState {
    fn new() -> Self {
        let rom_data = match std::env::var("ROM_FILE") {
            Ok(rom_file) => std::fs::read(&rom_file)
                .unwrap_or_else(|_| panic!("Could not read ROM {}", rom_file)),
            Err(_e) => include_bytes!("../assets/rom.nes").to_vec(),
        };

        let nes = load_rom(rom_data).expect("Failed to load ROM");

        Self { nes }
    }

    pub fn advance(&mut self, inputs: [&JoypadInput; MAX_PLAYERS]) {
        //println!("Advancing! {:?}", inputs);
        self.nes.p1_input = inputs[0].0;
        self.nes.p2_input = inputs[1].0;
        self.nes.run_until_vblank();
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
        self.nes.apu.consume_samples(); // clear buffer so we don't build up a delay
    }
}

struct GameRunner {
    state: MyGameState,
    sound_stream: Stream,
    pixels: Pixels,
    settings: Settings,
    inputs: Inputs,

    #[cfg(feature = "netplay")]
    netplay: network::Netplay,
}
impl GameRunner {
    pub fn new(pixels: Pixels, default_settings: &Settings) -> Self {
        let inputs = Inputs::new();
        let settings: Settings = Settings::new().unwrap_or_else(|err| {
            eprintln!("Failed to load config ({err}), falling back to default settings");
            default_settings.clone()
        });

        let audio = Audio::new();
        let sound_stream = audio.start(settings.audio.latency);
        let mut state = MyGameState::new();
        state
            .nes
            .apu
            .set_sample_rate(sound_stream.get_sample_rate());

        Self {
            state,
            sound_stream,
            pixels,
            settings,
            inputs,

            #[cfg(feature = "netplay")]
            netplay: network::Netplay::new(),
        }
    }
    pub fn advance(&mut self) -> Fps {
        #[allow(unused_assignments)]
        #[allow(unused_mut)]
        let mut fps = FPS;
        #[cfg(not(feature = "netplay"))]
        self.state.advance([&self.inputs.p1, &self.inputs.p2]);

        #[cfg(feature = "netplay")]
        {
            fps = self
                .netplay
                .advance(&mut self.state, [&self.inputs.p1, &self.inputs.p2]);
        }

        let sound_data = self.state.nes.apu.consume_samples();
        for sample in sound_data {
            self.sound_stream.push_sample(sample);
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
                                if let Err(err) = self.save_state() {
                                    eprintln!("Could not write save file: {}", err);
                                }
                            }
                            Some(VirtualKeyCode::F2) => {
                                if let Err(err) = self.load_state() {
                                    eprintln!("Could not read savefile: {}", err);
                                }
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

    fn save_state(&self) -> Result<()> {
        let mut file = File::create("save.bin")?;
        let data = self.state.save();
        file.write_all(&data)?;
        Ok(())
    }

    fn load_state(&mut self) -> Result<()> {
        let mut file = File::open("save.bin")?;
        let buf = &mut Vec::new();
        file.read_to_end(buf)?;

        self.state.load(buf);
        Ok(())
    }
}
