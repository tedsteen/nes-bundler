#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::gui::Gui;
use crate::audio::Audio;
use crate::joypad_mappings::JoypadMappings;

use std::ops::{Deref};
use std::sync::{Arc, Mutex};
use std::fs;

use game_loop::game_loop;
use ggrs::{GGRSEvent, GGRSRequest, GameState, NULL_FRAME, P2PSession, SessionState};

use egui_wgpu_backend::wgpu;
use log::error;
use p2p::{P2P};
use pixels::{Pixels, PixelsBuilder, SurfaceTexture};
use rusticnes_core::ppu::PpuState;
use tokio::runtime::Runtime;
use winit::dpi::LogicalSize;
use winit::event::{Event as WinitEvent, VirtualKeyCode};
use winit::event_loop::{EventLoop};
use winit::window::WindowBuilder;

use rusticnes_core::palettes::NTSC_PAL;
use rusticnes_core::nes::NesState;
use rusticnes_core::cartridge::mapper_from_file;
use rusticnes_core::mmc::none::NoneMapper;

use rust_embed::RustEmbed;

mod gui;
mod joypad_mappings;
mod audio;
mod discovery;
mod peer;
mod p2p;

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

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Asset;

const FPS: u32 = 60;
const INPUT_SIZE: usize = std::mem::size_of::<u8>();
const NUM_PLAYERS: usize = 2;
const WIDTH: u32 = 256;
const HEIGHT: u32 = 240;
const ZOOM: f32 = 1.5;

//#[tokio::main]
fn main() {
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

    let (pixels, gui) = {
        let window_size = window.inner_size();
        let scale_factor = window.scale_factor();
        let surface_texture = SurfaceTexture::new(window_size.width, window_size.height, &window);

        let pixels = PixelsBuilder::new(WIDTH, HEIGHT, surface_texture)
        .request_adapter_options(wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
        })
        .build().unwrap();

        let gui = Gui::new(window_size.width, window_size.height, scale_factor, &pixels);
        (pixels, gui)
    };

    let game = Game::new(gui, pixels);
    
    let audio = Audio::new();
    let mut audio_stream = audio.start(game.audio_latency, game.nes.clone());    

    game_loop(event_loop, window, game, FPS, 0.08, move |g| {
        let game = &mut g.game;
        
        if game.frames_to_skip > 0 {
            game.decrease_frames_to_skip();
            println!("Frame {} skipped: WaitRecommendation", game.frame);
            return;
        }
        //println!("State: {:?}", game.sess.current_state());
        //game.update(game.pad1.state, game.pad2.state);
        if game.sess.current_state() == SessionState::Running {
            match game.sess.advance_frame(game.local_handle, &vec![game.pad1.state]) {
                Ok(requests) => {
                    for request in requests {
                        match request {
                            GGRSRequest::LoadGameState { cell } => {
                                let game_state = cell.load();
                                let frame = game_state.frame;

                                println!("LOAD {}, diff in frame: {}", game_state.checksum, game.frame - frame);
                                let mut nes = game.nes.lock().unwrap();
                                game.frame = frame;
                                *nes = bincode::deserialize(game_state.buffer.unwrap().as_slice()).unwrap();
                            },
                            GGRSRequest::SaveGameState { cell, frame } => {
                                let nes = game.nes.lock().unwrap();
                                let state = bincode::serialize(nes.deref()).unwrap();
                                let game_state = GameState::new(frame, Some(state), None);
                                //println!("SAVE {}", game_state.checksum);
                                cell.save(game_state);
                            },
                            GGRSRequest::AdvanceFrame { inputs } => {
                                let pad1 = if inputs[0].frame == NULL_FRAME {
                                    0 //Disconnected player
                                } else {
                                    inputs[0].input()[0]
                                };
                                let pad2 = if inputs[1].frame == NULL_FRAME {
                                    0 //Disconnected player
                                } else {
                                    inputs[1].input()[0]
                                };
                                game.update(pad1, pad2);
                            },
                        }
                    }
                }
                Err(ggrs::GGRSError::PredictionThreshold) => {
                    println!(
                        "Frame {} skipped: PredictionThreshold",
                        game.frame
                    );
                }
                Err(e) => eprintln!("Ouch :( {:?}", e)
            }
            
            //regularily print networks stats
            if game.frame % 120 == 0 {
                for i in 0..NUM_PLAYERS {
                    if let Ok(stats) = game.sess.network_stats(i as usize) {
                        println!("NetworkStats to player {}: {:?}", i, stats);
                    }
                }
            }
        }
    }, move |g| {
        g.game.render(&g.window);
    }, move |g, event| {
        let game = &mut g.game;
        game.sess.poll_remote_clients();
        let mut a = 0;

        for event in game.sess.events() {
            if let GGRSEvent::WaitRecommendation { skip_frames } = event {
                a += skip_frames;
            }
            println!("Event: {:?}", event);
        }
        game.increase_frames_to_skip(a);
        
        if !g.game.handle(event) {
            g.exit();
        }
        audio_stream.set_latency(g.game.audio_latency);
    });
}

struct Game {
    frame: i32,
    frames_to_skip: u32,
    sess: P2PSession,
    local_handle: usize,
    gui: Gui,
    pixels: Pixels,
    audio_latency: u16,
    nes: Arc<Mutex<NesState>>,
    pad1: JoypadMappings,
    pad2: JoypadMappings,
    #[allow(dead_code)] // This reference needs to be held on to to keep the runtime going
    runtime: Runtime,
    #[allow(dead_code)] // This reference needs to be held on to to keep the P2P going
    p2p: P2P

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
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let (p2p, (mut sess, local_handle)) = runtime.block_on(async {
            let p2p = P2P::new().await;

            let p2p_game = p2p.create_game("private", NUM_PLAYERS).await;
            
            let s = p2p_game.start_session(INPUT_SIZE).await;
            (p2p, s)
        });

        //sess.set_sparse_saving(true).unwrap();
        sess.set_fps(FPS).unwrap();
        sess.set_frame_delay(4, local_handle).unwrap();
        sess.start_session().expect("Could not start P2P session");
        
        Self {
            frame: 0,
            frames_to_skip: 0,
            sess,
            local_handle,
            gui,
            pixels,
            audio_latency: 400,
            nes,
            pad1: JoypadMappings::DEFAULT_PAD1,
            pad2: JoypadMappings::DEFAULT_PAD2,
            runtime,
            p2p
        }
    }
    
    pub fn increase_frames_to_skip(self: &mut Self, frames: u32) {
        self.frames_to_skip += frames;
    }
    
    pub fn decrease_frames_to_skip(self: &mut Self) {
        self.frames_to_skip -= 1;
    }

    pub fn update(&mut self, pad1: u8, pad2: u8) {
        let mut nes = self.nes.lock().unwrap();

        nes.p1_input = pad1;
        nes.p2_input = pad2;
        nes.run_until_vblank();

        self.frame += 1;
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
                    use std::fs::File;
                    use std::io::{Read, Write};
                    match code {
                        VirtualKeyCode::Escape => {
                            if input.state == winit::event::ElementState::Pressed {
                                self.gui.show_gui = !self.gui.show_gui;
                            }
                        },
                        VirtualKeyCode::F1 => {
                            if input.state == winit::event::ElementState::Pressed {
                                let buffer = bincode::serialize(self.nes.lock().unwrap().deref()).unwrap();
                                let filename = "save.state";
                                let mut file = File::create(filename).unwrap();
                                file.write_all(buffer.as_slice()).expect("Failed to write save state");
                            }
                        },
                        VirtualKeyCode::F2 => {
                            if input.state == winit::event::ElementState::Pressed {
                                let filename = "save.state";
                                let mut file = File::open(&filename).expect("no file found");
                                let metadata = fs::metadata(&filename).expect("unable to read metadata");
                                let mut buffer = vec![0; metadata.len() as usize];
                                file.read(&mut buffer).expect("buffer overflow");
                                let old_nes: NesState = bincode::deserialize(buffer.as_slice()).unwrap();
                                let mut nes = self.nes.lock().expect("wat");
                                *nes = old_nes;
                            }
                        },
                        _ => {
                            self.pad1.apply(&input);
                            self.pad2.apply(&input);
                        }

                    }
                }
            }
        }
        true
    }
}