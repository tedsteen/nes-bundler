#![deny(clippy::all)]
#![forbid(unsafe_code)]

use std::process::exit;

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
mod netplay;
mod palette;
mod settings;
#[cfg(feature = "debug")]
mod debug;

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

#[derive(Deserialize)]
pub struct BuildConfiguration {
    window_title: String,
    default_settings: Settings,
    #[cfg(feature = "netplay")]
    netplay: netplay::NetplayBuildConfiguration,
}
fn main() {
    let build_config: BuildConfiguration =
        serde_yaml::from_str(include_str!("../config/build_config.yaml")).unwrap();

    if std::env::args().collect::<String>().contains(&"--print-netplay-id".to_string()) {
        if let Some(id) = build_config.netplay.netplay_id {
            println!("{id}");
        }
        exit(0);
    }

    let event_loop = EventLoop::new();

    let window = {
        WindowBuilder::new()
            .with_title(&build_config.window_title)
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

    let game_runner = GameRunner::new(pixels, &build_config);
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
                game_runner
                    .sound_stream
                    .set_volume(game_runner.settings.audio.volume);

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

pub struct MyGameState {
    nes: NesState,
}

impl MyGameState {
    fn new() -> Self {
        let rom_data = match std::env::var("ROM_FILE") {
            Ok(rom_file) => std::fs::read(&rom_file)
                .unwrap_or_else(|_| panic!("Could not read ROM {}", rom_file)),
            Err(_e) => include_bytes!("../config/rom.nes").to_vec(),
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
    pub fn new(pixels: Pixels, build_config: &BuildConfiguration) -> Self {
        let inputs = Inputs::new(build_config.default_settings.input.clone());
        let settings: Settings = Settings::new(&build_config.default_settings);

        let audio = Audio::new();
        let sound_stream = audio.start(&settings.audio);
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
            netplay: netplay::Netplay::new(&build_config.netplay),
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

        self.sound_stream.push_samples(self.state.nes.apu.consume_samples().as_slice());

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
