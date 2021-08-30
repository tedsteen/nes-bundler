#![deny(clippy::all)]
#![forbid(unsafe_code)]

extern crate rusticnes_core;
extern crate cpal;
extern crate ringbuf;

use std::rc::Rc;
use std::time::SystemTime;
use std::fs;

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

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::RingBuffer;

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

  pub fn set_audio_samplerate(runtime: &mut RuntimeState, sample_rate: u32) {
    let nes = &mut runtime.nes;
    nes.apu.set_sample_rate(sample_rate as u64);
  }

  pub fn set_audio_buffersize(runtime: &mut RuntimeState, buffer_size: u32) {
    let nes = &mut runtime.nes;
    nes.apu.set_buffer_size(buffer_size as usize);
  }

  pub fn audio_buffer_full(runtime: &mut RuntimeState) -> bool {
    let nes = &runtime.nes;
    return nes.apu.buffer_full;
  }

  pub fn get_audio_buffer(runtime: &mut RuntimeState) -> Vec<i16> {
    let nes = &mut runtime.nes;
    nes.apu.buffer_full = false;
    return nes.apu.output_buffer.to_owned();
  }

  fn main() -> Result<(), Error> {
    env_logger::init();
    let host = cpal::default_host();

    let output_device = host.default_output_device().expect("Could not get default output device");
    println!("Output device: {}", output_device.name().unwrap());
    let output_config = output_device.default_output_config().unwrap();
    println!("Default output config: {:?}", output_config);
    
    match output_config.sample_format() {
        cpal::SampleFormat::F32 => run::<f32>(&output_device, &output_config.into()),
        cpal::SampleFormat::I16 => run::<i16>(&output_device, &output_config.into()),
        cpal::SampleFormat::U16 => run::<u16>(&output_device, &output_config.into()),
    }
}

pub fn run<T>(output_device: &cpal::Device, config: &cpal::StreamConfig) -> Result<(), Error>
where
T: cpal::Sample,
{
    let mut runtime: RuntimeState = RuntimeState::new();
    let sample_rate = config.sample_rate.0 as f32;
    let channels = config.channels as usize;

    const LATENCY: f32 = 100.0;
    // Create a delay in case the input and output devices aren't synced.
    let latency_frames = (LATENCY / 1_000.0) * sample_rate as f32;
    let latency_samples = latency_frames as usize * channels as usize;
    
    load_rom(&mut runtime, fs::read("rom2.nes").expect("Could not read ROM").as_slice());
    set_audio_samplerate(&mut runtime, sample_rate as u32*2); //TODO: Why * 2??
    set_audio_buffersize(&mut runtime, (latency_samples) as u32); //TODO: Look into what is a good value
    
    println!("TED: {:?},{:?}", latency_frames, latency_samples);

    // The buffer to share samples
    let ring = RingBuffer::<f32>::new(latency_samples * 2);
    let (mut producer, mut consumer) = ring.split();

    // Fill the samples with 0.0 equal to the length of the delay.
    for _ in 0..latency_samples {
        producer.push(0.0).unwrap();
    }
    
    let output_data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
        let mut input_fell_behind = false;
        
        for sample in data {
            *sample = match consumer.pop() {
                Some(s) => s,
                None => {
                    input_fell_behind = true;
                    0.0
                }
            };
        }
        if input_fell_behind {
            eprintln!("input stream fell behind: try increasing latency");
        }
    };
    let output_stream = output_device.build_output_stream(&config, output_data_fn, err_fn).expect("Could not build sound output stream");

    output_stream.play().expect("Could not start playing output stream");

    let event_loop = EventLoop::new();
    let mut input = WinitInputHelper::new();
    let window = {
        let size = LogicalSize::new(WIDTH as f64, HEIGHT as f64);
        WindowBuilder::new()
            .with_title("Hello rusticnes!")
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

    //use futures::executor::ThreadPool;
    //let pool = ThreadPool::new().expect("Failed to build pool");
    //pool.spawn(async move {
    //}).expect("Spawn failed");
        
    let mut start_time = SystemTime::now();
    let mut current_frame = 0;

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
            let mut p1_input: u8 = 0;
            //let mut p2_input: u8 = 0;
            
            if input.key_held(VirtualKeyCode::Up) {
                p1_input |= 0b00010000u8;
            }
            if input.key_held(VirtualKeyCode::Down) {
                p1_input |= 0b00100000u8;
            }
            if input.key_held(VirtualKeyCode::Left) {
                p1_input |= 0b01000000u8;
            }
            if input.key_held(VirtualKeyCode::Right) {
                p1_input |= 0b10000000u8;
            }
            
            if input.key_held(VirtualKeyCode::Key1) {
                p1_input |= 0b10001000u8;
            }
            if input.key_held(VirtualKeyCode::Key2) {
                p1_input |= 0b00000100u8;
            }

            if input.key_held(VirtualKeyCode::LControl) {
                p1_input |= 0b00000010u8;
            }
            if input.key_held(VirtualKeyCode::LAlt) {
                p1_input |= 0b00000001u8;
            }

            
            runtime.nes.p1_input = p1_input;
            //runtime.nes.p2_input = p2_input;

            // Resize the window
            if let Some(size) = input.window_resized() {
                pixels.resize_surface(size.width, size.height);
            }
        }

        let runtime_in_ms = SystemTime::now().duration_since(start_time).expect("wat").as_millis();
        let target_frame = runtime_in_ms / (1000 / 60);
    
        if target_frame - current_frame > 2 {
            println!("We're running behind, reset the timer so we don't run off the deep end");
            println!("Running frame {:?}/{:?} ({:?})", current_frame, target_frame, (target_frame-current_frame));
            start_time = SystemTime::now();
            current_frame = 0;
            run_until_vblank(&mut runtime);
            //window.request_redraw();
        } else {
            while current_frame < target_frame {
                run_until_vblank(&mut runtime);
                current_frame += 1;
                window.request_redraw();
            }
            //TODO: Sleep a bit here if there's a long time until next target frame?
            //thread::sleep(std::time::Duration::from_millis(10));
        }

        if audio_buffer_full(&mut runtime) {
            let audio_buffer = get_audio_buffer(&mut runtime);
            for e in audio_buffer {
                let _r = producer.push(e as f32/ 32768.0); //TODO: Why / 32768.0?

            }
            //producer.push_slice(audio_buffer.as_slice());
        }
        
    });
}

fn err_fn(err: cpal::StreamError) {
    eprintln!("an error occurred on stream: {}", err);
}
