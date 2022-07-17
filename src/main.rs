#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::input::{JoypadInput};
use audio::{Audio, Stream};

use game_loop::game_loop;

use gui::Framework;
use input::{Inputs};
use log::error;
use palette::NTSC_PAL;
use pixels::{Pixels, SurfaceTexture};
use rusticnes_core::cartridge::mapper_from_file;
use rusticnes_core::nes::NesState;
use settings::{Settings, MAX_PLAYERS};
use winit::dpi::LogicalSize;
use winit::event::{Event, VirtualKeyCode};
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

mod audio;
mod gui;
mod input;
mod palette;
mod settings;
#[cfg(feature = "netplay")]
mod network;

const FPS: u32 = 60;
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

async fn async_main() {
    env_logger::init();

    let event_loop = EventLoop::new();

    let window = {
        WindowBuilder::new()
            .with_title("Hello rusticnes!")
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

    let game_runner = GameRunner::new(pixels);

    game_loop(
        event_loop,
        window,
        (game_runner, framework),
        FPS,
        0.08,
        move |g| {
            let game_runner = &mut g.game.0;            
            game_runner.advance();
            
            #[cfg(feature = "netplay")]
            if game_runner.netplay.run_slow {
                g.set_updates_per_second((FPS as f32 * 0.9) as u32 )
            } else {
                g.set_updates_per_second(FPS)
            }
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

pub(crate) struct MyGameState {
    nes: NesState
}

impl MyGameState {
    fn new() -> Self {
        let rom_data = match std::env::var("ROM_FILE") {
            Ok(rom_file) => std::fs::read(&rom_file)
                .unwrap_or_else(|_| panic!("Could not read ROM {}", rom_file)),
            Err(_e) => include_bytes!("../assets/rom.nes").to_vec()
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
        let mut data = vec!();
        data.extend(self.nes.save_state());
        data
    }
    fn load(&mut self, data: &mut Vec<u8>) {
        self.nes.load_state(data);
        self.nes.apu.consume_samples(); // clear buffer so we don't build up a delay
    }

}

struct GameRunner {
    state: MyGameState,
    audio: Audio,
    sound_stream: Stream,
    pixels: Pixels,
    settings: Settings,
    inputs: Inputs,

    #[cfg(feature = "netplay")]
    netplay: network::Netplay
}

impl GameRunner {
    pub fn new(pixels: Pixels) -> Self {
        let inputs = Inputs::new();
        let settings: Settings = Default::default();

        let audio = Audio::new();
        let sound_stream = audio.start(settings.audio_latency, None);
        let mut my_state = MyGameState::new();
        my_state.nes.apu.set_sample_rate(sound_stream.sample_rate as u64);

        Self {
            state: my_state,
            audio,
            sound_stream,
            pixels,
            settings,
            inputs,

            #[cfg(feature = "netplay")]
            netplay: network::Netplay::new()
        }
    }
    pub fn advance(&mut self) {
        if self.sound_stream.get_latency() != self.settings.audio_latency {
            self.sound_stream = self.audio.start(self.settings.audio_latency, Some(&mut self.sound_stream));
            //clear buffer
            self.state.nes.apu.consume_samples();
        }

        #[cfg(not(feature = "netplay"))]
        self.state.advance([self.inputs.p1, self.inputs.p2]);

        #[cfg(feature = "netplay")]
        self.netplay.advance(&mut self.state, [&self.inputs.p1, &self.inputs.p2]);

        let sound_data = self.state.nes.apu.consume_samples();
        for sample in sound_data {
            let _ = self.sound_stream.producer.push(sample);
        }
    }

    pub fn render(&mut self, window: &winit::window::Window, gui_framework: &mut Framework) {
        gui_framework.prepare(window, self);

        let pixels = &mut self.pixels;

        let frame = pixels.get_frame();
        self.state.render(frame);

        // Render everything together
        let render_result = pixels.render_with(|encoder, render_target, context| {
            // Render the world texture
            context.scaling_renderer.render(encoder, render_target);

            // Render egui
            gui_framework.render(encoder, render_target, context);

            Ok(())
        });
        if render_result.map_err(|e| error!("pixels.render() failed: {}", e)).is_err() {
            //TODO: what to do here?
        }
    }

    pub fn handle(&mut self, event: &winit::event::Event<()>, gui_framework: &mut Framework) -> bool {
        self.inputs.advance(event, &mut self.settings);
        // Handle input events
        if let Event::WindowEvent { event, .. } = event {
            match event {
                winit::event::WindowEvent::CloseRequested => {
                    return false;
                },
                winit::event::WindowEvent::ScaleFactorChanged{ scale_factor, new_inner_size: _ } => {
                    gui_framework.scale_factor(*scale_factor);
                },
                winit::event::WindowEvent::Resized(size) => {
                    self.pixels.resize_surface(size.width, size.height);
                    gui_framework.resize(size.width, size.height)
                },
                winit::event::WindowEvent::KeyboardInput { input, .. } => {
                    if input.state == winit::event::ElementState::Pressed {
                        match input.virtual_keycode {
                            Some(VirtualKeyCode::F1) => {
                                let data = self.state.save();
                                let _ = std::fs::remove_file("save.bin");
                                if let Err(err) = std::fs::write("save.bin", data) {
                                    eprintln!("Could not write save file: {:?}", err);
                                }
                            }
                            Some(VirtualKeyCode::F2) => {
                                match std::fs::read("save.bin") {
                                    Ok(mut bytes) => {
                                        self.state.load(&mut bytes);
                                    },
                                    Err(err) =>  eprintln!("Could not read savefile: {:?}", err)
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
}
