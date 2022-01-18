#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::input::{JoypadInput, StaticJoypadInput};
#[cfg(feature = "netplay")]
use crate::network::p2p::P2P;

use audio::{Audio, Stream};
use game_loop::game_loop;
#[cfg(feature = "netplay")]
use ggrs::{GGRSEvent, GGRSRequest, GameState, P2PSession, SessionState, NULL_FRAME};

use egui_wgpu_backend::wgpu;
use gui::Gui;
use input::{JoypadKeyMap, JoypadKeyboardInput};
use log::error;
use pixels::{Pixels, PixelsBuilder, SurfaceTexture};
use rusticnes_core::cartridge::mapper_from_file;
use rusticnes_core::nes::NesState;
use rusticnes_core::palettes::NTSC_PAL;
use winit::dpi::LogicalSize;
use winit::event::{Event as WinitEvent, VirtualKeyCode};
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

mod audio;
mod gui;
mod input;
#[cfg(feature = "netplay")]
mod network;

const FPS: u32 = 60;
#[cfg(feature = "netplay")]
const INPUT_SIZE: usize = std::mem::size_of::<u8>();
const MAX_PLAYERS: usize = 4;
const WIDTH: u32 = 256;
const HEIGHT: u32 = 240;
const ZOOM: f32 = 3.0;

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
        let gui = Gui::new(&window, &pixels, #[cfg(feature = "netplay")] P2P::new(INPUT_SIZE).await);
        
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

struct MyGameState {
    nes: NesState,
    sound_stream: Stream,
}

impl MyGameState {
    fn new(audio_latency: u16) -> Self {
        let rom_data = match std::env::var("ROM_FILE") {
            Ok(rom_file) => std::fs::read(&rom_file)
                .unwrap_or_else(|_| panic!("Could not read ROM {}", rom_file)),
            Err(_e) => include_bytes!("../assets/rom.nes").to_vec()
        };

        let mut nes = load_rom(rom_data).expect("Failed to load ROM");

        let audio = Audio::new();
        let sound_stream = audio.start(audio_latency);
        nes.apu.set_sample_rate(sound_stream.sample_rate as u64);

        Self { nes, sound_stream }
    }

    pub fn load(&mut self, data: &[u8]) {
        self.nes.load_state(&mut data.to_vec());
        self.nes.apu.consume_samples(); //Clear audio buffer so we don't build up a delay
        self.sound_stream.drain();
    }

    pub fn save(&self) -> Vec<u8> {
        self.nes.save_state()
    }

    pub fn advance(&mut self, inputs: Vec<StaticJoypadInput>) {
        //println!("Advancing! {:?}", inputs);
        self.nes.p1_input = inputs[0].to_u8();
        self.nes.p2_input = inputs[1].to_u8();
        self.nes.run_until_vblank();

        let apu = &mut self.nes.apu;
        for sample in apu.consume_samples() {
            if self.sound_stream.producer.push(sample).is_err() {
                //eprintln!("Sound buffer full");
            }
        }
    }

    fn render(&self, frame: &mut [u8]) {
        let screen = &self.nes.ppu.screen;

        for (i, pixel) in frame.chunks_exact_mut(4).enumerate() {
            let palette_index = screen[i] as usize * 3;
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
#[cfg(feature = "netplay")]
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
    #[cfg(feature = "netplay")]
    NetPlay(NetPlayState),
}
enum GameRunnerState {
    Playing(MyGameState, PlayState),
    #[allow(dead_code)] // Calm down.. it's coming..
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
        let audio_latency = 20;
        Self {
            state: GameRunnerState::Playing(MyGameState::new(audio_latency), PlayState::LocalPlay()),
            gui,
            pixels,
            settings: Settings {
                audio_latency,
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
                game_state
                .sound_stream
                .set_latency(self.settings.audio_latency, &mut game_state.nes);

                match play_state {
                    PlayState::LocalPlay() => {
                        let a = self
                            .settings
                            .inputs
                            .iter()
                            .map(|inputs| match inputs.selected {
                                SelectedInput::Keyboard => {
                                    StaticJoypadInput(inputs.get_pad().to_u8())
                                }
                            })
                            .collect();
                        game_state.advance(a);
                    }
                    #[cfg(feature = "netplay")]
                    PlayState::NetPlay(netplay_state) => {
                        //TODO: Somewhere somehow do `session.poll_remote_clients()`
                        
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
                        if sess.current_state() == SessionState::Running {
                            match sess.advance_frame(
                                netplay_state.local_handle,
                                &[self.settings.inputs[0].get_pad().to_u8()],
                            ) {
                                Ok(requests) => {
                                    for request in requests {
                                        match request {
                                            GGRSRequest::LoadGameState { cell, frame: _ } => {
                                                let g_s = cell.load();
                                                let frame = g_s.frame;
                                                game_state.load(&g_s.data.expect("No data in cell?"));
                                                println!(
                                                    "LOAD {}, diff in frame: {}",
                                                    g_s.checksum,
                                                    netplay_state.frame - frame
                                                );
                                                netplay_state.frame = frame; //TODO: Look into this frame stuff here...
                                            }
                                            GGRSRequest::SaveGameState { cell, frame } => {
                                                let state = game_state.save();
                                                let game_state = GameState::new(frame, Some(state));
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
        gui.prepare(window, &mut self.settings, #[cfg(feature = "netplay")] &mut self.state);

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
        // Handle input events
        if let WinitEvent::WindowEvent { event, .. } = event {
            // Update egui inputs
            self.gui.handle_event(&event, &mut self.settings);

            if let winit::event::WindowEvent::Resized(size) = event {
                self.pixels.resize_surface(size.width, size.height);
            }

            if let winit::event::WindowEvent::KeyboardInput { input, .. } = event {
                if let Some(code) = input.virtual_keycode {
                    if input.state == winit::event::ElementState::Pressed {
                        match code {
                            VirtualKeyCode::Escape => {
                                self.gui.show_gui = !self.gui.show_gui;
                            }
                            VirtualKeyCode::F1 => {
                                if let GameRunnerState::Playing(game_state, _) = &mut self.state {
                                    let data = game_state.save();
                                    let _ = std::fs::remove_file("save.bin");
                                    if let Err(err) = std::fs::write("save.bin", data) {
                                        eprintln!("Could not write save file: {:?}", err);
                                    }
                                }
                            }
                            VirtualKeyCode::F2 => {
                                if let GameRunnerState::Playing(game_state, _) = &mut self.state {
                                    match std::fs::read("save.bin") {
                                        Ok(bytes) => { game_state.load(&bytes); },
                                        Err(err) =>  eprintln!("Could not read savefile: {:?}", err)
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                for joypad_inputs in &mut self.settings.inputs {
                    joypad_inputs.keyboard.apply(&input);
                }
            }
        }
        true
    }
}
