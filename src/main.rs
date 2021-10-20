#![deny(clippy::all)]
#![forbid(unsafe_code)]

use std::collections::HashMap;

use crate::input::{JoypadInput, StaticJoypadInput};
use crate::network::p2p::{P2P};

use game_loop::game_loop;
use ggrs::{GGRSEvent, GGRSRequest, GameState, NULL_FRAME, P2PSession, SessionState};

use egui_wgpu_backend::wgpu;
use gui::Gui;
use input::{JoypadKeyMap, JoypadKeyboardInput};
use log::error;
use pixels::{Pixels, PixelsBuilder, SurfaceTexture};
use winit::dpi::LogicalSize;
use winit::event::{Event as WinitEvent, VirtualKeyCode};
use winit::event_loop::{EventLoop};
use winit::window::WindowBuilder;

mod gui;
mod input;
mod audio;
mod network;

const FPS: u32 = 60;
const INPUT_SIZE: usize = std::mem::size_of::<u8>();
const NUM_PLAYERS: usize = 2;
const WIDTH: u32 = 256;
const HEIGHT: u32 = 240;
const ZOOM: f32 = 2.0;

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(long)]
    game_name: String,
    #[structopt(long)]
    create: bool,
    #[structopt(long, required_if("create", "true"), default_value = "2")]
    slots: u8
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
        let scale_factor = window.scale_factor();
        let surface_texture = SurfaceTexture::new(window_size.width, window_size.height, &window);

        let pixels = PixelsBuilder::new(WIDTH, HEIGHT, surface_texture)
        .request_adapter_options(wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
        })
        .build().unwrap();

        let gui = Gui::new(window_size.width, window_size.height, scale_factor, &pixels, P2P::new(INPUT_SIZE).await);
        (pixels, gui)
    };

    let game_runner = GameRunner::new(gui, pixels).await;
    
    game_loop(event_loop, window, game_runner, FPS, 0.08, move |g| {
        let game_runner = &mut g.game;
        game_runner.advance();
    }, move |g| {
        let game_runner = &mut g.game;
        game_runner.render(&g.window);
    }, move |g, event| {
        let game_runner = &mut g.game;
        if !game_runner.handle(event) {
            g.exit();
        }
    });
}

struct MyBox {
    x: f64,
    y: f64,
    x_vel: f64,
    y_vel: f64
}

struct MyGameState {
    boxes: HashMap<usize, MyBox>
}

impl MyGameState {
    fn new() -> Self {
        Self {
            boxes: HashMap::new()
        }
    }

    //TODO: Use a Vec<JoypadInput> instead?
    pub fn advance(&mut self, inputs: Vec<StaticJoypadInput>) {
        //println!("Advancing! {:?}", inputs);
        use input::JoypadButton::*;
        for (idx, input) in inputs.iter().enumerate() {
            let b = self.get_box(idx);
            if input.is_pressed(UP) {
                b.y_vel -= 0.1;
            }
            if input.is_pressed(DOWN) {
                b.y_vel += 0.1;
            }
            if input.is_pressed(LEFT) {
                b.x_vel -= 0.1;
            }
            if input.is_pressed(RIGHT) {
                b.x_vel += 0.1;
            }
            b.x_vel *= 0.98;
            b.y_vel *= 0.98;

            b.x += b.x_vel;
            b.y += b.y_vel;

            if b.x + BOX_SIZE > WIDTH as f64 { b.x = WIDTH as f64 - BOX_SIZE; b.x_vel *= -1.0 }
            if b.x < 0.0 { b.x = 0.0; b.x_vel *= -1.0 }
            if b.y + BOX_SIZE > HEIGHT as f64 { b.y = HEIGHT as f64 - BOX_SIZE; b.y_vel *= -1.0 }
            if b.y < 0.0 { b.y = 0.0; b.y_vel *= -1.0 }
        }
    }

    fn get_box(&mut self, idx: usize) -> &mut MyBox {
        self.boxes.entry(idx).or_insert_with(|| { MyBox { x: WIDTH as f64 / 2.0 - BOX_SIZE / 2.0, y: HEIGHT as f64 / 2.0 - BOX_SIZE / 2.0, x_vel: 0.0, y_vel: 0.0 } })
    }
}

enum SelectedInput {
    Keyboard
}

struct JoypadInputs {
    selected: SelectedInput,
    keyboard: JoypadKeyboardInput
}

struct Settings {
    audio_latency: u16,
    inputs: Vec<JoypadInputs>
}

impl JoypadInputs {
    fn get_pad(self: &Self) -> Box<&dyn JoypadInput> {
        match self.selected {
            SelectedInput::Keyboard => Box::new(&self.keyboard),
        }
    }
}
struct NetPlayState {
    session: P2PSession,
    local_handle: usize,
    frames_to_skip: u32,
    frame: i32
}

impl NetPlayState {
/*
    async fn new() -> Self {

        
        let p2p_game = if opt.create {
            println!("Creating game {}", opt.game_name);
            
            println!("Created!");
            game
        } else {
            loop {
                if let Some(owner_id) = p2p.find_owner(&opt.game_name).await {
                    println!("Joining game '{}' ({:?}...", opt.game_name, owner_id);
                    let game = p2p.join_game(owner_id).await;
                    println!("Joined!");
                    break game;
                } else {
                    println!("Looking for game '{}'", opt.game_name);
                }
                tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            }
        };

        let (mut session, local_handle) = loop {
            match p2p_game.current_state().await {
                network::p2p::GameState::Initializing => {
                    println!("Initializing...");
                },
                network::p2p::GameState::New(slots) => {
                    println!("Waiting for slots to be occupied and connected: {:?}", slots);

                    let vacant_idx = slots.iter().enumerate()
                    .find(|(_, slot) | matches!(slot, Slot::Vacant()))
                    .map(|(idx, _)| idx);

                    if let Some(vacant_idx) = vacant_idx {
                        let our_slot = slots.iter()
                        .find(|slot| matches!(slot, Slot::Occupied(Participant::Local(_))));
                        if our_slot.is_none() {
                            p2p_game.claim_slot(vacant_idx).await;    
                        }
                    }
                },
                network::p2p::GameState::Ready(ready_state) => {
                    let mut peers = Vec::new();
                    for participant in ready_state.players.clone() {
                        if let Participant::Remote(peer_id, _) = participant {
                            peers.push(p2p.get_peer(peer_id).await);
                        }
                    }
                    if !peers.iter().all(|peer| matches!(*peer.connection_state.borrow(), PeerState::Connected(_)) ) {
                        println!("Waiting for all peers to connect...");
                    } else {
                        println!("Starting session!");
                        break p2p.start_session(ready_state, INPUT_SIZE).await;
                    }
                },
            }
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        };
        //sess.set_sparse_saving(true).unwrap();
        session.set_fps(FPS).unwrap();
        session.set_frame_delay(4, local_handle).unwrap();
        session.start_session().expect("Could not start P2P session");
        
        Self {
            frame: 0,
            session,
            local_handle,
            frames_to_skip: 0
        }
    }
*/
}
enum PlayState {
    LocalPlay(),
    NetPlay(NetPlayState)
}
enum GameRunnerState {
    Loading(String, f64),
    Playing(MyGameState, PlayState)
}
struct GameRunner {
    state: GameRunnerState,
    gui: Gui,
    pixels: Pixels,
    settings: Settings
}

const BOX_SIZE: f64 = 20.0;
impl GameRunner {
    pub async fn new(gui: Gui, pixels: Pixels) -> Self {
        Self {
            state: GameRunnerState::Playing(MyGameState::new(), PlayState::LocalPlay()),
            gui,
            pixels,
            settings: Settings {
                audio_latency: 50,
                inputs: vec!(
                    JoypadInputs {
                        selected: SelectedInput::Keyboard,
                        keyboard: JoypadKeyboardInput::new(JoypadKeyMap::default_pad1())
                    },
                    JoypadInputs {
                        selected: SelectedInput::Keyboard,
                        keyboard: JoypadKeyboardInput::new(JoypadKeyMap::default_pad2())
                    }
                )
            }
        }
    }
    
    pub fn advance(self: &mut Self) {
        let state = &mut self.state;
        match state {
            GameRunnerState::Loading(msg, progress) => {
                //TODO: ...
                println!("Loading: \"{}\", ({}%)", msg, *progress * 100.0);
            },
            GameRunnerState::Playing(game_state, play_state) => {
                match play_state {
                    PlayState::LocalPlay() => {
                        let a = self.settings.inputs.iter().map(|inputs| {
                            match inputs.selected {
                                SelectedInput::Keyboard => StaticJoypadInput(inputs.keyboard.to_u8()),
                            }
                        }).collect();
                        game_state.advance(a);
                    },
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
                        //game.update(game.pad1.state, game.pad2.state);
                        if sess.current_state() == SessionState::Running {
                            let pad1 = self.settings.inputs[0].get_pad();
                
                            match sess.advance_frame(netplay_state.local_handle, &vec![pad1.to_u8()]) {
                                Ok(requests) => {
                                    for request in requests {
                                        match request {
                                            GGRSRequest::LoadGameState { cell } => {
                                                let game_state = cell.load();
                                                let frame = game_state.frame;
                
                                                println!("LOAD {}, diff in frame: {}", game_state.checksum, netplay_state.frame - frame);
                                                netplay_state.frame = frame; //TODO: Look into this frame stuff here...
                                            },
                                            GGRSRequest::SaveGameState { cell, frame } => {
                                                let nes = &0;
                
                                                let state = bincode::serialize(nes).unwrap();
                                                let game_state = GameState::new(frame, Some(state), None);
                                                //println!("SAVE {}", game_state.checksum);
                                                cell.save(game_state);
                                            },
                                            GGRSRequest::AdvanceFrame { inputs } => {
                                                let inputs = inputs.iter().map(|i| {
                                                    if i.frame == NULL_FRAME {
                                                        StaticJoypadInput(0) //disconnected player
                                                    } else {
                                                        StaticJoypadInput(i.buffer[0])
                                                    }
                                                }).collect();
                                                game_state.advance(inputs);
                                            },
                                        }
                                    }
                                }
                                Err(ggrs::GGRSError::PredictionThreshold) => {
                                    println!("Frame {} skipped: PredictionThreshold", netplay_state.frame);
                                }
                                Err(e) => eprintln!("Ouch :( {:?}", e)
                            }
                            
                            //regularily print networks stats
                            if netplay_state.frame % 120 == 0 {
                                for i in 0..NUM_PLAYERS {
                                    if let Ok(stats) = sess.network_stats(i as usize) {
                                        println!("NetworkStats to player {}: {:?}", i, stats);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn render(&mut self, window: &winit::window::Window) -> bool {
        let pixels = &mut self.pixels;
        
        if let GameRunnerState::Playing(game_state, _) = &self.state {
            let frame = pixels.get_frame();

            for (i, pixel) in frame.chunks_exact_mut(4).enumerate() {
                let x = (i % WIDTH as usize) as f64;
                let y = (i / WIDTH as usize) as f64;
                
                let rgba = game_state.boxes.values().enumerate().find(|(_, b)| {
                    x >= b.x
                    && x < b.x + BOX_SIZE
                    && y >= b.y
                    && y < b.y + BOX_SIZE
                }).map_or([0x00, 0x00, 0x00, 0x00], |(a, _)| {
                    if a == 0 {
                        [0x48, 0xb2, 0xe8, 0xff]
                    } else if a == 1 {
                        [0x5e, 0x48, 0xe8, 0xff]
                    } else {
                        [0xff, 0xff, 0xff, 0xff]
                    }
                });
    
                pixel.copy_from_slice(&rgba);
            }
        }

        let gui = &mut self.gui;
        gui.prepare(&window, &mut self.settings, &mut self.state);

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
        self.gui.handle_event(&event, &mut self.settings);

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
                            self.settings.inputs[0].keyboard.apply(&input);
                            self.settings.inputs[1].keyboard.apply(&input);
                        }
                    }
                }
            }
        }
        true
    }
}