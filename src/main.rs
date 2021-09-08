#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::gui::Gui;
use crate::audio::Audio;
use crate::joypad_mappings::JoypadMappings;

use std::rc::Rc;
use std::time::SystemTime;

use egui_wgpu_backend::wgpu;
use log::error;
use pixels::{Error, PixelsBuilder, SurfaceTexture};
use winit::dpi::LogicalSize;
use winit::event::{Event as WinitEvent, VirtualKeyCode};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;
use winit_input_helper::WinitInputHelper;

use rusticnes_core::palettes::NTSC_PAL;
use rusticnes_ui_common::application::RuntimeState;
use rusticnes_ui_common::events::Event;

mod gui;
mod joypad_mappings;
mod audio;

pub fn dispatch_event(runtime: &mut RuntimeState, event: Event) -> Vec<Event> {
    let mut responses: Vec<Event> = Vec::new();
    responses.extend(runtime.handle_event(event.clone()));
    return responses;
}

pub fn resolve_events(runtime: &mut RuntimeState, mut events: Vec<Event>) {
    while events.len() > 0 {
        let event = events.remove(0);
        let responses = dispatch_event(runtime, event);
        events.extend(responses);
    }
}

pub fn load_rom(runtime: &mut RuntimeState, cart_data: &[u8]) {
    let mut events: Vec<Event> = Vec::new();
    let bucket_of_nothing: Vec<u8> = Vec::new();
    let cartridge_data = cart_data.to_vec();
    events.push(Event::LoadCartridge("cartridge".to_string(), Rc::new(cartridge_data), Rc::new(bucket_of_nothing)));
    resolve_events(runtime, events);
}

pub fn run_until_vblank(runtime: &mut RuntimeState) {
    let mut events: Vec<Event> = Vec::new();
    events.push(Event::NesRunFrame);
    resolve_events(runtime, events);
}

pub fn render_screen_pixels(runtime: &mut RuntimeState, frame: &mut [u8]) {
    let nes = &runtime.nes;

    for x in 0 .. 256 {
        for y in 0 .. 240 {
            let palette_index = ((nes.ppu.screen[y * 256 + x]) as usize) * 3;
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
        
    let mut runtime: RuntimeState = RuntimeState::new();
    load_rom(&mut runtime, &Asset::get("rom2.nes").expect("Missing embedded ROM").data);
    //use std::fs; load_rom(&mut runtime, fs::read("assets/rom2.nes").expect("Could not read ROM").as_slice());    

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

    let mut pad1 = JoypadMappings::default_pad1();
    let mut pad2 = JoypadMappings::default_pad2();
    let audio = Audio::new();
    
    let mut audio_stream = audio.start(100, 1);
    runtime.nes.apu.set_sample_rate(audio_stream.sample_rate as u64);
    runtime.nes.apu.set_buffer_size(audio_stream.buffer_length / 2); //TODO: Look into what is a good value, should prob be less than the ring buffer
    
    let (mut start_time, mut current_frame, mut nes_redraw_req) = (SystemTime::now(), 0, false);
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        // Update egui inputs
        gui.handle_event(&event);

        // Handle input events
        if input.update(&event) {
            // Close events
            if input.key_pressed(VirtualKeyCode::Escape) || input.quit() {
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

            if input.key_pressed(VirtualKeyCode::F1) {
                gui.show_gui = !gui.show_gui;
            }

            runtime.nes.p1_input = pad1.to_pad(&input);
            runtime.nes.p2_input = pad2.to_pad(&input);
        }

        match event {
            WinitEvent::MainEventsCleared => {
                let runtime_in_ms = SystemTime::now().duration_since(start_time).unwrap().as_millis();
                let target_nes_frame = (runtime_in_ms as f64 / (1000.0 / 60.0)) as u128;
                
                if target_nes_frame - current_frame > 2 {
                    eprintln!("We're running behind, reset the timer so we don't run off the deep end (frame {:?}/{:?})", current_frame, target_nes_frame);
                    start_time = SystemTime::now();
                    current_frame = 0;
                    run_until_vblank(&mut runtime);
                    nes_redraw_req = true;
                } else {
                    while current_frame < target_nes_frame {
                        run_until_vblank(&mut runtime);
                        nes_redraw_req = true;
                        current_frame += 1;
                    }
                }
                window.request_redraw();
                if runtime.nes.apu.buffer_full {
                    runtime.nes.apu.buffer_full = false;
                    let audio_buffer = runtime.nes.apu.output_buffer.to_owned();
                    let result = audio_stream.producer.push_slice(audio_buffer.as_slice());
                    if result < audio_buffer.len() {
                        eprintln!("Producing audio faster than it's being consumed! ({:?} left)", audio_buffer.len() - result);
                    }
                }
            },
            WinitEvent::RedrawRequested(_) => {
                if nes_redraw_req {
                    render_screen_pixels(&mut runtime, pixels.get_frame());
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