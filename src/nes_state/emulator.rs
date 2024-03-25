use std::sync::{Arc, Mutex};

use crate::bundle;
use crate::window::VideoFramePool;
use crate::{
    audio::Audio,
    fps::RateCounter,
    gameloop::GameLoop,
    input::{Inputs, JoypadState},
    settings::{gui::GuiComponent, MAX_PLAYERS},
    Fps, FPS,
};
use anyhow::Result;

use crate::nes_state::NesStateHandler;

pub struct Emulator {
    pub frame_pool: VideoFramePool,
    #[cfg(feature = "netplay")]
    nes_state: Arc<Mutex<crate::netplay::NetplayStateHandler>>,
    #[cfg(not(feature = "netplay"))]
    nes_state: Arc<Mutex<crate::nes_state::LocalNesState>>,
    debug: Arc<Mutex<EmulatorDebug>>,
    joypads: Arc<Mutex<[JoypadState; MAX_PLAYERS]>>,
}
struct EmulatorDebug {
    pub override_fps: bool,
    pub fps: Fps,
}
impl Emulator {
    pub fn start(inputs: &Inputs, audio: &mut Audio) -> Result<Self> {
        #[cfg(not(feature = "netplay"))]
        let nes_state = crate::nes_state::LocalNesState::load_rom(&bundle().rom);

        #[cfg(feature = "netplay")]
        let nes_state = { crate::netplay::NetplayStateHandler::new() };
        let debug = EmulatorDebug {
            override_fps: false,
            fps: FPS,
        };

        let mut audio_tx = audio.stream.start()?;
        let frame_pool = VideoFramePool::new();
        let shared_state = Emulator {
            frame_pool: frame_pool.clone(),
            nes_state: Arc::new(Mutex::new(nes_state)),
            debug: Arc::new(Mutex::new(debug)),
            joypads: inputs.joypads.clone(),
        };

        tokio::spawn({
            let debug = shared_state.debug.clone();
            let mut game_loop = GameLoop::new(shared_state.nes_state.clone(), FPS);
            let joypads = shared_state.joypads.clone();
            async move {
                let mut loop_counter = RateCounter::new();
                loop {
                    loop_counter.tick("LPS");

                    game_loop.next_frame(|game_loop| {
                        loop_counter.tick("FPS");
                        let nes_state = &mut game_loop.game;
                        let joypads = joypads.lock().unwrap();
                        let mut frame_data = match frame_pool.push_with(|video_frame| {
                            nes_state
                                .lock()
                                .unwrap()
                                .advance([joypads[0], joypads[1]], &mut Some(video_frame))
                        }) {
                            Ok(frame_data) => frame_data,
                            Err(_) => {
                                loop_counter.tick("DFPS");
                                //log::warn!("Frame dropped");
                                nes_state
                                    .lock()
                                    .unwrap()
                                    .advance([joypads[0], joypads[1]], &mut None)
                            }
                        };

                        if let Some(frame_data) = &mut frame_data {
                            //TODO: Testa detta -> audio_tx.push_iter(&mut frame_data.audio.drain(..audio_tx.free_len()));

                            audio_tx.push_slice(&frame_data.audio);
                            let debug = debug.lock().unwrap();
                            let fps = if debug.override_fps {
                                debug.fps
                            } else {
                                frame_data.fps
                            };
                            game_loop.set_updates_per_second(fps);
                        }
                    });
                    //sleep(Duration::from_millis(15)).await;
                    tokio::task::yield_now().await;
                    if let Some(report) = loop_counter.report() {
                        log::debug!("Emulator: {report}");
                    }
                }
            }
        });
        Ok(shared_state)
    }
    pub fn save_state(&self) -> Option<Vec<u8>> {
        self.nes_state.lock().unwrap().save()
    }

    pub fn load_state(&mut self, data: &mut Vec<u8>) {
        self.nes_state.lock().unwrap().load(data);
    }
}

pub struct EmulatorGui {
    #[cfg(feature = "netplay")]
    netplay_gui: crate::netplay::gui::NetplayGui,
}

impl EmulatorGui {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "netplay")]
            netplay_gui: crate::netplay::gui::NetplayGui::new(bundle().config.netplay.clone()),
        }
    }
}
#[cfg(not(feature = "netplay"))]
impl GuiComponent<Emulator> for EmulatorGui {}

#[cfg(feature = "netplay")]
impl GuiComponent<Emulator> for EmulatorGui {
    fn ui(&mut self, instance: &mut Emulator, ui: &mut egui::Ui) {
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
                    ui.add(egui::Slider::new(&mut debug.fps, 0.5..=180.0).suffix("FPS"));
                }
                ui.end_row();
            });
    }

    fn messages(&self, instance: &Emulator) -> Option<Vec<String>> {
        self.netplay_gui
            .messages(&instance.nes_state.lock().unwrap())
    }

    fn name(&self) -> Option<String> {
        self.netplay_gui.name()
    }

    fn prepare(&mut self, instance: &mut Emulator) {
        self.netplay_gui
            .prepare(&mut instance.nes_state.lock().unwrap());
    }
}
