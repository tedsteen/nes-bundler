#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::input::{JoypadInput, StaticJoypadInput};
use audio::{Audio, Stream};

use game_loop::game_loop;

use gui::Framework;
use log::error;
use palette::NTSC_PAL;
use pixels::{Pixels, SurfaceTexture};
use rusticnes_core::cartridge::mapper_from_file;
use rusticnes_core::nes::NesState;
use settings::{Settings, SelectedInput};
use winit::dpi::LogicalSize;
use winit::event::{Event as WinitEvent, VirtualKeyCode};
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

    let game_runner = GameRunner::new(framework, pixels);

    game_loop(
        event_loop,
        window,
        game_runner,
        FPS,
        0.08,
        move |g| {
            let game_runner = &mut g.game;
            let inputs = game_runner
                .settings
                .inputs
                .iter()
                .map(|inputs| match inputs.selected {
                    SelectedInput::Keyboard => {
                        StaticJoypadInput(inputs.get_pad().to_u8())
                    }
                })
                .collect();
            
            game_runner.advance(inputs);

            if game_runner.run_slow {
                g.set_updates_per_second((FPS as f32 * 0.9) as u32 )
            } else {
                g.set_updates_per_second(FPS)
            }
        },
        move |g| {
            let game_runner = &mut g.game;
            game_runner.render(&g.window);
        },
        move |g, event| {
            let game_runner = &mut g.game;
            if !game_runner.handle(event) {
                g.exit();
            }
        },
    );
}

pub(crate) type Frame = i32;

pub(crate) struct MyGameState {
    nes: NesState,
    frame: Frame
}

impl MyGameState {
    fn new() -> Self {
        let rom_data = match std::env::var("ROM_FILE") {
            Ok(rom_file) => std::fs::read(&rom_file)
                .unwrap_or_else(|_| panic!("Could not read ROM {}", rom_file)),
            Err(_e) => include_bytes!("../assets/rom.nes").to_vec()
        };

        let nes = load_rom(rom_data).expect("Failed to load ROM");

        Self { nes, frame: 0 }
    }

    pub fn advance(&mut self, inputs: Vec<StaticJoypadInput>, sound_stream: &mut Stream) {
        self.frame += 1;
        //println!("Advancing! {:?}", inputs);
        self.nes.p1_input = inputs[0].to_u8();
        self.nes.p2_input = inputs[1].to_u8();
        self.nes.run_until_vblank();
        let sound_data = self.nes.apu.consume_samples();
        for sample in sound_data {
            if sound_stream.producer.push(sample).map_err(|e| error!("sound_stream.producer.push(...) failed: {}", e)).is_err() {
                //Not much to do
            }
        }
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
        data.extend(self.frame.to_le_bytes());
        data.extend(self.nes.save_state());
        data
    }
    fn load(&mut self, data: &mut Vec<u8>) {
        self.nes.load_state(data);
        self.frame = i32::from_le_bytes(data.split_off(data.len() - 4).try_into().unwrap());
        self.nes.apu.consume_samples(); // clear buffer so we don't build up a delay
    }

}

struct GameRunner {
    state: MyGameState,
    run_slow: bool,
    sound_stream: Stream,
    gui_framework: Framework,
    pixels: Pixels,
    settings: Settings,
}

impl GameRunner {
    pub fn new(gui_framework: Framework, pixels: Pixels) -> Self {
        let settings: Settings = Default::default();

        let audio = Audio::new();
        let sound_stream = audio.start(settings.audio_latency);
        let mut my_state = MyGameState::new();
        my_state.nes.apu.set_sample_rate(sound_stream.sample_rate as u64);

        Self {
            state: my_state,
            sound_stream,
            run_slow: false,
            gui_framework,
            pixels,
            settings
        }
    }
    pub fn advance(&mut self, inputs: Vec<StaticJoypadInput>) {
        self.sound_stream.set_latency(self.settings.audio_latency, &mut self.state.nes);
        #[cfg(not(feature = "netplay"))]
        self.state.advance(inputs, &mut self.sound_stream);

        #[cfg(feature = "netplay")]
        network::advance(self, inputs)
    }

    pub fn render(&mut self, window: &winit::window::Window) {
        let pixels = &mut self.pixels;

        let frame = pixels.get_frame();
        self.state.render(frame);

        let gui_framework = &mut self.gui_framework;
        gui_framework.prepare(window, &mut self.settings);

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

    pub fn handle(&mut self, event: &winit::event::Event<()>) -> bool {
        // Handle input events
        if let WinitEvent::WindowEvent { event, .. } = event {
            match event {
                winit::event::WindowEvent::CloseRequested => {
                    return false;
                },
                winit::event::WindowEvent::ScaleFactorChanged{ scale_factor, new_inner_size: _ } => {
                    self.gui_framework.scale_factor(*scale_factor);
                },
                winit::event::WindowEvent::Resized(size) => {
                    self.pixels.resize_surface(size.width, size.height);
                    self.gui_framework.resize(size.width, size.height)
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
                    for joypad_inputs in &mut self.settings.inputs {
                        joypad_inputs.keyboard.apply(input);
                    }
                }
                _ => {}
            }

            // Update egui inputs
            self.gui_framework.handle_event(event, &mut self.settings);
        }
        true
    }
}
