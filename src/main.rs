#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::gui::Gui;
use crate::audio::Audio;
use crate::joypad_mappings::JoypadMappings;

use std::sync::{Arc, Mutex};
use std::fs;

use game_loop::game_loop;

use egui_wgpu_backend::wgpu;
use log::error;
use pixels::{Error, Pixels, PixelsBuilder, SurfaceTexture};
use rusticnes_core::ppu::PpuState;
use winit::dpi::LogicalSize;
use winit::event::{Event as WinitEvent, VirtualKeyCode};
use winit::event_loop::{EventLoop};
use winit::window::WindowBuilder;

use rusticnes_core::palettes::NTSC_PAL;
use rusticnes_core::nes::NesState;
use rusticnes_core::cartridge::mapper_from_file;
use rusticnes_core::mmc::none::NoneMapper;

mod gui;
mod joypad_mappings;
mod audio;

pub fn load_rom(cart_data: Vec<u8>) -> Result<NesState, String> {
    match mapper_from_file(cart_data.as_slice()) {
        Ok(mapper) => {
            let mut nes = NesState::new(mapper);
            nes.power_on();
            Ok(nes)
        },
        err => err.map(|_| NesState::new(Box::new(NoneMapper::new())))
    }
}

pub fn render_screen_pixels(ppu: &mut PpuState, frame: &mut [u8]) {
    for x in 0 .. 256 {
        for y in 0 .. 240 {
            let palette_index = ((ppu.screen[y * 256 + x]) as usize) * 3;
            let pixel_offset = (y * 256 + x) * 4;
            frame[pixel_offset + 0] = NTSC_PAL[palette_index + 0];
            frame[pixel_offset + 1] = NTSC_PAL[palette_index + 1];
            frame[pixel_offset + 2] = NTSC_PAL[palette_index + 2];
            frame[((y * 256 + x) * 4) + 3] = 255;
        }
    }
}

use rust_embed::RustEmbed;
#[derive(RustEmbed)]
#[folder = "assets/"]
struct Asset;

const FPS: u32 = 60;

fn main() -> Result<(), Error> {
    env_logger::init();
    let event_loop = EventLoop::new();

    let (width, height, zoom) = (256, 240, 3);
    let window = {
        WindowBuilder::new()
            .with_title("Hello rusticnes!")
            .with_inner_size(LogicalSize::new(width * zoom, height * zoom))
            .with_min_inner_size(LogicalSize::new(width, height))
            .build(&event_loop)
            .unwrap()
    };

    let (pixels, gui) = {
        let window_size = window.inner_size();
        let scale_factor = window.scale_factor();
        let surface_texture = SurfaceTexture::new(window_size.width, window_size.height, &window);

        let pixels = PixelsBuilder::new(width, height, surface_texture)
        .request_adapter_options(wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
        })
        .build()?;

        let gui = Gui::new(window_size.width, window_size.height, scale_factor, &pixels);
        (pixels, gui)
    };

    let game = Game::new(gui, pixels);
    let audio = Audio::new();
    let mut audio_stream = audio.start(game.audio_latency, game.nes.clone());

    game_loop(event_loop, window, game, FPS, 0.08, |g| {
        g.game.update();
        
    }, move |g| {
        g.game.render(&g.window);

    }, move |g, event| {
        if !g.game.handle(event) {
            g.exit();
        }
        audio_stream.set_latency(g.game.audio_latency);

    });
}

struct Game {
    gui: Gui,
    pixels: Pixels,
    audio_latency: u16,
    nes: Arc<Mutex<NesState>>,
    pad1: JoypadMappings,
    pad2: JoypadMappings
}

impl Game {
    pub fn new(gui: Gui, pixels: Pixels) -> Self {
        let rom_data = match std::env::var("ROM_FILE") {
            Ok(rom_file) => {
                let data = fs::read(&rom_file).expect(format!("Could not read ROM {}", rom_file).as_str());
                data
            },
            Err(_e) => Asset::get("rom.nes").expect("Missing embedded ROM").data.into_owned()
        };
    
        let nes = Arc::new(Mutex::new(load_rom(rom_data).expect("Failed to load ROM")));
        
        Self {
            gui,
            pixels,
            audio_latency: 100,
            nes,
            pad1: JoypadMappings::DEFAULT_PAD1,
            pad2: JoypadMappings::DEFAULT_PAD2
        }
    }

    pub fn update(&mut self) {
        self.nes.lock().unwrap().run_until_vblank();
    }

    pub fn render(&mut self, window: &winit::window::Window) -> bool {
        let pixels = &mut self.pixels;
        let gui = &mut self.gui;
        gui.prepare(&window, &mut self.pad1, &mut self.pad2, &mut self.audio_latency);

        //Render nes
        render_screen_pixels(&mut self.nes.lock().unwrap().ppu, pixels.get_frame());

        // Render everything together
        pixels.render_with(|encoder, render_target, context| {
            // Render the world texture
            let result = context.scaling_renderer.render(encoder, render_target);
            // Render egui
            gui.render(encoder, render_target, context).expect("GUI failed to render");

            result
        })
        .map_err(|e| error!("pixels.render() failed: {}", e))
        .is_err()
    }

    pub fn handle(&mut self, event: winit::event::Event<()>) -> bool {
        // Update egui inputs
        self.gui.handle_event(&event);

        // Handle input events
        if let WinitEvent::WindowEvent { event, .. } = event {
            if let winit::event::WindowEvent::Resized(size) = event {
                self.pixels.resize_surface(size.width, size.height);
                self.gui.resize(size.width, size.height);
            }
            if let winit::event::WindowEvent::ScaleFactorChanged{scale_factor, ..} = event {
                self.gui.scale_factor(scale_factor);
            }

            if let winit::event::WindowEvent::KeyboardInput { input, .. } = event {
                if let Some(code) = input.virtual_keycode {
                    match code {
                        VirtualKeyCode::Escape => {
                            if input.state == winit::event::ElementState::Pressed {
                                self.gui.show_gui = !self.gui.show_gui;
                            }
                        },
                        _ => {
                            let nes = &mut self.nes.lock().unwrap();
                            nes.p1_input = self.pad1.to_pad(&input);
                            nes.p2_input = self.pad2.to_pad(&input);
                        }
                        
                    }
                }
            }
        }
        true
    }
}