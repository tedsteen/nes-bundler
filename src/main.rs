#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::input::{JoypadInput, StaticJoypadInput};
use crate::network::p2p::P2P;

use game_loop::game_loop;
use ggrs::{GGRSEvent, GGRSRequest, GameState, P2PSession, SessionState, NULL_FRAME};

use egui_wgpu_backend::wgpu;
use gui::Gui;
use input::{JoypadKeyMap, JoypadKeyboardInput};
use log::error;
use pixels::{Pixels, PixelsBuilder, SurfaceTexture};
use rusticnes_core::cartridge::mapper_from_file;
use rusticnes_core::mmc::none::NoneMapper;
use rusticnes_core::nes::NesState;
use rusticnes_core::palettes::NTSC_PAL;
use winit::dpi::LogicalSize;
use winit::event::{Event as WinitEvent, VirtualKeyCode};
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

mod audio;
mod gui;
mod input;
mod network;

const FPS: u32 = 60;
const INPUT_SIZE: usize = std::mem::size_of::<u8>();
const MAX_PLAYERS: usize = 4;
const WIDTH: u32 = 256;
const HEIGHT: u32 = 240;
const ZOOM: f32 = 2.0;

use rust_embed::RustEmbed;
#[derive(RustEmbed)]
#[folder = "assets/"]
struct Asset;

pub fn load_rom(cart_data: Vec<u8>) -> Result<NesState, String> {
    match mapper_from_file(cart_data.as_slice()) {
        Ok(mapper) => {
            let mut nes = NesState::new(mapper);
            nes.power_on();
            Ok(nes)
        }
        err => err.map(|_| NesState::new(Box::new(NoneMapper::new()))),
    }
}

#[tokio::main]
async fn main() {
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
        let surface_texture = SurfaceTexture::new(window_size.width, window_size.height, &window);

        let pixels = PixelsBuilder::new(WIDTH, HEIGHT, surface_texture)
            .request_adapter_options(wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .build()
            .unwrap();

        let gui = Gui::new(&window, &pixels, P2P::new(INPUT_SIZE).await);
        (pixels, gui)
    };

    let game_runner = GameRunner::new(gui, pixels).await;

    game_loop(
        event_loop,
        window,
        game_runner,
        FPS,
        0.08,
        move |g| {
            let game_runner = &mut g.game;
            game_runner.advance();
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

use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
struct MyGameState {
    nes: NesState,
}

impl MyGameState {
    fn new() -> Self {
        let rom_data = match std::env::var("ROM_FILE") {
            Ok(rom_file) => {
                
                std::fs::read(&rom_file)
                    .unwrap_or_else(|_| panic!("Could not read ROM {}", rom_file))
            }
            Err(_e) => Asset::get("rom.nes")
                .expect("Missing embedded ROM")
                .data
                .into_owned(),
        };

        let nes = load_rom(rom_data).expect("Failed to load ROM");

        Self { nes }
    }

    pub fn advance(&mut self, inputs: Vec<StaticJoypadInput>) {
        //println!("Advancing! {:?}", inputs);
        self.nes.p1_input = inputs[0].to_u8();
        self.nes.p2_input = inputs[1].to_u8();
        self.nes.run_until_vblank();
    }

    fn render(&self, frame: &mut [u8]) {
        let ppu = &self.nes.ppu;
        for (i, pixel) in frame.chunks_exact_mut(4).enumerate() {
            let palette_index = ppu.screen[i] as usize * 3;
            let rgba = &NTSC_PAL[palette_index..palette_index + 4]; //TODO: cheating with the alpha channel here..
            pixel.copy_from_slice(rgba);
        }
    }
}

enum SelectedInput {
    Keyboard,
}

struct JoypadInputs {
    selected: SelectedInput,
    keyboard: JoypadKeyboardInput,
}

struct Settings {
    audio_latency: u16,
    inputs: [JoypadInputs; MAX_PLAYERS],
}

impl JoypadInputs {
    fn get_pad(&self) -> &dyn JoypadInput {
        match self.selected {
            SelectedInput::Keyboard => &self.keyboard,
        }
    }
}
struct NetPlayState {
    session: P2PSession,
    player_count: usize,
    local_handle: usize,
    frames_to_skip: u32,
    frame: i32,
}

#[allow(clippy::large_enum_variant)]
enum PlayState {
    LocalPlay(),
    NetPlay(NetPlayState),
}
enum GameRunnerState {
    Playing(MyGameState, PlayState),
    Loading,
}
struct GameRunner {
    state: GameRunnerState,
    gui: Gui,
    pixels: Pixels,
    settings: Settings,
}

impl GameRunner {
    pub async fn new(gui: Gui, pixels: Pixels) -> Self {
        Self {
            state: GameRunnerState::Playing(MyGameState::new(), PlayState::LocalPlay()),
            gui,
            pixels,
            settings: Settings {
                audio_latency: 50,
                inputs: [
                    JoypadInputs {
                        selected: SelectedInput::Keyboard,
                        keyboard: JoypadKeyboardInput::new(JoypadKeyMap::default_pad1()),
                    },
                    JoypadInputs {
                        selected: SelectedInput::Keyboard,
                        keyboard: JoypadKeyboardInput::new(JoypadKeyMap::default_pad2()),
                    },
                    JoypadInputs {
                        selected: SelectedInput::Keyboard,
                        keyboard: JoypadKeyboardInput::new(JoypadKeyMap::unmapped()),
                    },
                    JoypadInputs {
                        selected: SelectedInput::Keyboard,
                        keyboard: JoypadKeyboardInput::new(JoypadKeyMap::unmapped()),
                    },
                ],
            },
        }
    }

    pub fn advance(&mut self) {
        let state = &mut self.state;
        match state {
            GameRunnerState::Playing(game_state, play_state) => {
                match play_state {
                    PlayState::LocalPlay() => {
                        let a = self
                            .settings
                            .inputs
                            .iter()
                            .map(|inputs| match inputs.selected {
                                SelectedInput::Keyboard => {
                                    StaticJoypadInput(inputs.keyboard.to_u8())
                                }
                            })
                            .collect();
                        game_state.advance(a);
                    }
                    PlayState::NetPlay(netplay_state) => {
                        netplay_state.frame += 1;
                        let sess = &mut netplay_state.session;
                        sess.poll_remote_clients();

                        for event in sess.events() {
                            if let GGRSEvent::WaitRecommendation { skip_frames } = event {
                                netplay_state.frames_to_skip += skip_frames;
                            }
                            println!("Event: {:?}", event);
                        }

                        if netplay_state.frames_to_skip > 0 {
                            netplay_state.frames_to_skip -= 1;
                            println!("Frame {} skipped: WaitRecommendation", netplay_state.frame);
                            return;
                        }

                        //println!("State: {:?}", game.sess.current_state());
                        if sess.current_state() == SessionState::Running {
                            match sess.advance_frame(
                                netplay_state.local_handle,
                                &[self.settings.inputs[0].get_pad().to_u8()],
                            ) {
                                Ok(requests) => {
                                    for request in requests {
                                        match request {
                                            GGRSRequest::LoadGameState { cell } => {
                                                let g_s = cell.load();
                                                let frame = g_s.frame;
                                                *game_state =
                                                    bincode::deserialize(&g_s.buffer.unwrap())
                                                        .unwrap();
                                                println!(
                                                    "LOAD {}, diff in frame: {}",
                                                    g_s.checksum,
                                                    netplay_state.frame - frame
                                                );
                                                netplay_state.frame = frame; //TODO: Look into this frame stuff here...
                                            }
                                            GGRSRequest::SaveGameState { cell, frame } => {
                                                let state = bincode::serialize(game_state).unwrap();
                                                let game_state =
                                                    GameState::new(frame, Some(state), None);
                                                //println!("SAVE {}", game_state.checksum);
                                                cell.save(game_state);
                                            }
                                            GGRSRequest::AdvanceFrame { inputs } => {
                                                let inputs = inputs
                                                    .iter()
                                                    .map(|i| {
                                                        if i.frame == NULL_FRAME {
                                                            StaticJoypadInput(0)
                                                        //disconnected player
                                                        } else {
                                                            StaticJoypadInput(i.buffer[0])
                                                        }
                                                    })
                                                    .collect();
                                                game_state.advance(inputs);
                                            }
                                        }
                                    }
                                }
                                Err(ggrs::GGRSError::PredictionThreshold) => {
                                    println!(
                                        "Frame {} skipped: PredictionThreshold",
                                        netplay_state.frame
                                    );
                                }
                                Err(e) => eprintln!("Ouch :( {:?}", e),
                            }

                            //regularily print networks stats
                            if netplay_state.frame % 120 == 0 {
                                for i in 0..netplay_state.player_count {
                                    if let Ok(stats) = sess.network_stats(i as usize) {
                                        println!("NetworkStats to player {}: {:?}", i, stats);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            GameRunnerState::Loading => todo!(),
        }
    }

    pub fn render(&mut self, window: &winit::window::Window) -> bool {
        let pixels = &mut self.pixels;

        if let GameRunnerState::Playing(game_state, _) = &self.state {
            let frame = pixels.get_frame();
            game_state.render(frame);
        }

        let gui = &mut self.gui;
        gui.prepare(window, &mut self.settings, &mut self.state);

        // Render everything together
        pixels
            .render_with(|encoder, render_target, context| {
                // Render the world texture
                context.scaling_renderer.render(encoder, render_target);
                // Render egui
                gui.render(encoder, render_target, context)
                    .expect("GUI failed to render");
                Ok(())
            })
            .map_err(|e| error!("pixels.render() failed: {}", e))
            .is_err()
    }

    pub fn handle(&mut self, event: winit::event::Event<()>) -> bool {
        if let GameRunnerState::Playing(_, PlayState::NetPlay(netplay_state)) = &mut self.state {
            netplay_state.session.poll_remote_clients(); //TODO: Is this a good idea?..
        }

        // Handle input events
        if let WinitEvent::WindowEvent { event, .. } = event {
            // Update egui inputs
            self.gui.handle_event(&event, &mut self.settings);

            if let winit::event::WindowEvent::Resized(size) = event {
                self.pixels.resize_surface(size.width, size.height);
            }

            if let winit::event::WindowEvent::KeyboardInput { input, .. } = event {
                if let Some(code) = input.virtual_keycode {
                    match code {
                        VirtualKeyCode::Escape => {
                            if input.state == winit::event::ElementState::Pressed {
                                self.gui.show_gui = !self.gui.show_gui;
                            }
                        }
                        _ => {
                            for joypad_inputs in &mut self.settings.inputs {
                                joypad_inputs.keyboard.apply(&input);
                            }
                        }
                    }
                }
            }
        }
        true
    }
}
