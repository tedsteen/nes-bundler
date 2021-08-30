#![deny(clippy::all)]
#![forbid(unsafe_code)]

extern crate rusticnes_core;

use std::rc::Rc;

use log::error;
use pixels::{Error, Pixels, SurfaceTexture};
use winit::dpi::LogicalSize;
use winit::event::{Event as WinitEvent, VirtualKeyCode};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;
use winit_input_helper::WinitInputHelper;

use rusticnes_core::palettes::NTSC_PAL;
use rusticnes_ui_common::application::RuntimeState;
use rusticnes_ui_common::events::Event;

const WIDTH: u32 = 256;
const HEIGHT: u32 = 240;

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

  fn main() -> Result<(), Error> {
    let mut runtime: RuntimeState = RuntimeState::new();
    use std::fs;
    load_rom(&mut runtime, fs::read("rom.nes").expect("Could not read ROM").as_slice());

    env_logger::init();
    let event_loop = EventLoop::new();
    let mut input = WinitInputHelper::new();
    let window = {
        let size = LogicalSize::new(WIDTH as f64, HEIGHT as f64);
        WindowBuilder::new()
            .with_title("Hello Pixels")
            .with_inner_size(size)
            .with_min_inner_size(size)
            .build(&event_loop)
            .unwrap()
    };

    let mut pixels = {
        let window_size = window.inner_size();
        let surface_texture = SurfaceTexture::new(window_size.width, window_size.height, &window);
        Pixels::new(WIDTH, HEIGHT, surface_texture)?
    };

    event_loop.run(move |event, _, control_flow| {
        // Draw the current frame
        if let WinitEvent::RedrawRequested(_) = event {
            //println!("render");
            render_screen_pixels(&mut runtime, pixels.get_frame());
            
            if pixels
                .render()
                .map_err(|e| error!("pixels.render() failed: {}", e))
                .is_err()
            {
                *control_flow = ControlFlow::Exit;
                return;
            }
        }

        // Handle input events
        if input.update(&event) {
            // Close events
            if input.key_pressed(VirtualKeyCode::Escape) || input.quit() {
                *control_flow = ControlFlow::Exit;
                return;
            }

            // Resize the window
            if let Some(size) = input.window_resized() {
                pixels.resize_surface(size.width, size.height);
            }

            // Update internal state and request a redraw
            run_until_vblank(&mut runtime);
            //println!("request redraw");
            window.request_redraw();
        }
    });
}