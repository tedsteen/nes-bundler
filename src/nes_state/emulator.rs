use std::sync::{Arc, Mutex};

use crate::bundle;
use crate::nes_state::NesStateHandler;
use crate::{
    audio::{Audio, AudioSender},
    fps::RateCounter,
    gameloop::GameLoop,
    input::{sdl2_impl::Sdl2Gamepads, Inputs, JoypadState},
    netplay::{gui::NetplayGui, NetplayBuildConfiguration, NetplayStateHandler},
    settings::{gui::GuiComponent, MAX_PLAYERS},
    window::egui_winit_wgpu::VideoFramePool,
    Fps, FPS,
};
use anyhow::Result;
use egui::Slider;
use ringbuf::HeapRb;
use sdl2::Sdl;
use tokio::task::JoinHandle;

pub struct SharedState {
    nes_state: Arc<Mutex<NetplayStateHandler>>,
    debug: Arc<Mutex<EmulatorDebug>>,
    joypads: Arc<Mutex<[JoypadState; MAX_PLAYERS]>>,
}
pub struct Emulator {
    audio_tx: Option<AudioSender>,
    pub shared_state: SharedState,
    pub inputs: Inputs,
    pub audio: Audio,
    netplay_config: NetplayBuildConfiguration,
    frame_pool: VideoFramePool,
}
struct EmulatorDebug {
    // Debug
    pub override_fps: bool,
    pub fps: Fps,
}
impl Emulator {
    pub fn new(frame_pool: VideoFramePool, sdl_context: &Sdl) -> Result<Self> {
        //TODO: Figure out a good buffer here..
        let (audio_tx, audio_rx) = HeapRb::<f32>::new(1024 * 8).split();

        let audio = Audio::new(sdl_context, audio_rx)?;

        let inputs = Inputs::new(Sdl2Gamepads::new(
            sdl_context.game_controller().map_err(anyhow::Error::msg)?,
        ));

        #[cfg(not(feature = "netplay"))]
        let nes_state = crate::nes_state::LocalNesState::load_rom(&bundle().rom);

        #[cfg(feature = "netplay")]
        let nes_state = { crate::netplay::NetplayStateHandler::new() };
        let debug = EmulatorDebug {
            override_fps: false,
            fps: FPS,
        };

        Ok(Self {
            shared_state: SharedState {
                nes_state: Arc::new(Mutex::new(nes_state)),
                debug: Arc::new(Mutex::new(debug)),
                joypads: inputs.joypads.clone(),
            },
            netplay_config: bundle().config.netplay.clone(),
            audio,
            inputs,
            frame_pool,
            audio_tx: Some(audio_tx),
        })
    }

    pub fn start(&mut self) -> Result<JoinHandle<()>> {
        if let Some(mut audio_tx) = self.audio_tx.take() {
            Ok(tokio::spawn({
                let debug = self.shared_state.debug.clone();
                let mut game_loop = GameLoop::new(self.shared_state.nes_state.clone(), FPS);
                let frame_pool = self.frame_pool.clone();
                let joypads = self.shared_state.joypads.clone();
                async move {
                    let mut loop_counter = RateCounter::new();
                    loop {
                        loop_counter.tick("LPS");

                        //println!("LOOP");
                        game_loop.next_frame(|game_loop| {
                            //println!("FRAME");
                            if let Some(report) = loop_counter.tick("FPS").report() {
                                println!("{report}");
                            }
                            let _ = frame_pool.push_with(|video_frame| {
                                let nes_state = &mut game_loop.game;
                                let joypads = joypads.lock().unwrap();

                                let mut frame_data = nes_state
                                    .lock()
                                    .unwrap()
                                    .advance([joypads[0], joypads[1]], video_frame);
                                if let Some(frame_data) = &mut frame_data {
                                    //TODO: Testa detta -> audio_tx.push_iter(&mut frame_data.audio.drain(..audio_tx.free_len()));

                                    audio_tx.push_slice(&frame_data.audio);
                                    //println!("AUDIO LENGTH: {}", audio_tx.len());
                                    let debug = debug.lock().unwrap();
                                    let fps = if debug.override_fps {
                                        debug.fps
                                    } else {
                                        frame_data.fps
                                    };
                                    game_loop.set_updates_per_second(fps);
                                }
                            });
                        });
                        //sleep(Duration::from_millis(15)).await;
                        tokio::task::yield_now().await
                    }
                }
            }))
        } else {
            Err(anyhow::format_err!("Already started"))
        }
    }
    pub fn save_state(&self) -> Option<Vec<u8>> {
        self.shared_state.nes_state.lock().unwrap().save()
    }

    pub fn load_state(&mut self, data: &mut Vec<u8>) {
        self.shared_state.nes_state.lock().unwrap().load(data);
    }

    pub fn new_gui(&self) -> EmulatorGui {
        EmulatorGui::new(NetplayGui::new(self.netplay_config.clone()))
    }
}

#[allow(dead_code)]
pub struct EmulatorGui {
    netplay_gui: NetplayGui,
}

impl EmulatorGui {
    fn new(netplay_gui: NetplayGui) -> Self {
        Self { netplay_gui }
    }
}
impl GuiComponent<SharedState> for EmulatorGui {
    fn ui(&mut self, instance: &mut SharedState, ui: &mut egui::Ui) {
        self.netplay_gui
            .ui(&mut instance.nes_state.lock().unwrap(), ui);

        #[cfg(feature = "debug")]
        egui::Grid::new("debug_grid")
            .num_columns(2)
            .spacing([10.0, 4.0])
            .striped(true)
            .show(ui, |ui| {
                let mut debug = instance.debug.lock().unwrap();
                ui.checkbox(&mut debug.override_fps, "Override FPS");
                if debug.override_fps {
                    ui.add(Slider::new(&mut debug.fps, 0.5..=180.0).suffix("FPS"));
                }
                ui.end_row();
            });
    }

    fn messages(&self, instance: &SharedState) -> Option<Vec<String>> {
        self.netplay_gui
            .messages(&instance.nes_state.lock().unwrap())
    }

    fn name(&self) -> Option<String> {
        self.netplay_gui.name()
    }
}
