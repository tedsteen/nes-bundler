#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::gui::Gui;
use crate::audio::Audio;
use crate::joypad_mappings::JoypadMappings;

use std::sync::atomic::Ordering;
use std::time::SystemTime;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;
use std::fs;


use egui_wgpu_backend::wgpu;
use log::error;
use pixels::{Error, PixelsBuilder, SurfaceTexture};
use winit::dpi::LogicalSize;
use winit::event::{Event as WinitEvent, VirtualKeyCode};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;
use winit_input_helper::WinitInputHelper;

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

pub fn render_screen_pixels(nes: &mut NesState, frame: &mut [u8]) {
    let ppu = &nes.ppu;

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

fn main() -> Result<(), Error> {
    env_logger::init();

    let rom_data = match std::env::var("ROM_FILE") {
        Ok(rom_file) => {
            let data = fs::read(&rom_file).expect(format!("Could not read ROM {}", rom_file).as_str());
            data
        },
        Err(_e) => Asset::get("rom.nes").expect("Missing embedded ROM").data.into_owned()
    };

    let nes = Arc::new(Mutex::new(load_rom(rom_data).expect("Failed to load ROM")));
    
    let event_loop = EventLoop::new();
    let mut input = WinitInputHelper::new();

    let (width, height, zoom) = (256, 240, 3);
    let window = {
        WindowBuilder::new()
            .with_title("Hello rusticnes!")
            .with_inner_size(LogicalSize::new(width * zoom, height * zoom))
            .with_min_inner_size(LogicalSize::new(width, height))
            .build(&event_loop)
            .unwrap()
    };

    let (mut pixels, mut gui) = {
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

    let mut pad1 = JoypadMappings::DEFAULT_PAD1;
    let mut pad2 = JoypadMappings::DEFAULT_PAD2;
    let audio = Audio::new();

    let mut audio_stream = audio.start(gui.latency, nes.clone());

    let exit = Arc::new(AtomicBool::new(false));
    {
        let exit = Arc::clone(&exit);
        ctrlc::set_handler(move || {
            exit.swap(true, Ordering::Relaxed);
        }).expect("Error setting Ctrl-C handler");
    }

    let (mut start_time, mut current_frame, mut nes_redraw_req) = (SystemTime::now(), 0, false);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        if exit.load(Ordering::Relaxed) {
            *control_flow = ControlFlow::Exit;
            return;
        }

        // Update egui inputs
        gui.handle_event(&event);
        audio_stream.set_latency(gui.latency);
        // Handle input events
        if input.update(&event) {
            // Close events
            if input.quit() {
                *control_flow = ControlFlow::Exit;
                return;
            }
            // Update the scale factor
            if let Some(scale_factor) = input.scale_factor() {
                gui.scale_factor(scale_factor);
            }
            // Resize the window
            if let Some(size) = input.window_resized() {
                pixels.resize_surface(size.width, size.height);
                gui.resize(size.width, size.height);
            }

            if input.key_pressed(VirtualKeyCode::Escape) {
                gui.show_gui = !gui.show_gui;
            }
            let nes = &mut nes.lock().unwrap();
            nes.p1_input = pad1.to_pad(&input);
            nes.p2_input = pad2.to_pad(&input);
        }

        match event {
            WinitEvent::MainEventsCleared => {
                let runtime_in_ms = SystemTime::now().duration_since(start_time).unwrap().as_millis();
                let target_nes_frame = (runtime_in_ms as f64 / (1000.0 / 60.0)) as u128;

                if target_nes_frame - current_frame > 2 {
                    eprintln!("We're running behind, reset the timer so we don't run off the deep end (frame {:?}/{:?})", current_frame, target_nes_frame);
                    start_time = SystemTime::now();
                    current_frame = 0;
                    nes.lock().unwrap().run_until_vblank();
                    nes_redraw_req = true;
                } else {
                    while current_frame < target_nes_frame {
                        nes.lock().unwrap().run_until_vblank();
                        nes_redraw_req = true;
                        current_frame += 1;
                    }
                }
                window.request_redraw();
            },
            WinitEvent::RedrawRequested(_) => {
                if nes_redraw_req {
                    render_screen_pixels(&mut nes.lock().unwrap(), pixels.get_frame());
                }

                gui.prepare(&window, &mut pad1, &mut pad2);

                // Render everything together
                let render_result = pixels.render_with(|encoder, render_target, context| {
                    // Render the world texture
                    let result = context.scaling_renderer.render(encoder, render_target);

                    // Render egui
                    gui.render(encoder, render_target, context).expect("GUI failed to render");

                    result
                });

                // Basic error handling
                if render_result
                    .map_err(|e| error!("pixels.render() failed: {}", e))
                    .is_err()
                {
                    *control_flow = ControlFlow::Exit;
                    return;
                }
            },
            _ => ()
        }
    });
}