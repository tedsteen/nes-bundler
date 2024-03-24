use std::sync::{Arc, Mutex};

use egui::Slider;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::{
    audio::AudioSender,
    bundle::Bundle,
    fps::RateCounter,
    gameloop::GameLoop,
    input::JoypadState,
    netplay::{gui::NetplayGui, NetplayBuildConfiguration, NetplayStateHandler},
    settings::{gui::GuiComponent, Settings, MAX_PLAYERS},
    window::egui_winit_wgpu::VideoFramePool,
    Fps, FPS,
};

use super::NesStateHandler;

pub struct Emulator {
    _jh: JoinHandle<()>,
    nes_state: Arc<Mutex<NetplayStateHandler>>,
    debug: Arc<Mutex<EmulatorDebug>>,
    netplay_config: NetplayBuildConfiguration,
}
struct EmulatorDebug {
    // Debug
    pub override_fps: bool,
    pub fps: Fps,
}
impl Emulator {
    pub fn new(
        bundle: &Bundle,
        settings: &mut Settings,
        frame_pool: VideoFramePool,
        mut audio_tx: AudioSender,
        joypads: Arc<Mutex<[JoypadState; MAX_PLAYERS]>>,
    ) -> Self {
        let rom = bundle.rom.clone();

        #[cfg(not(feature = "netplay"))]
        let nes_state = crate::nes_state::LocalNesState::load_rom(&rom);

        #[cfg(feature = "netplay")]
        let nes_state = {
            let netplay_id = settings
                .netplay_id
                .get_or_insert_with(|| Uuid::new_v4().to_string())
                .to_string();
            let netplay_rom = bundle.netplay_rom.clone();
            crate::netplay::NetplayStateHandler::new(rom, netplay_rom, netplay_id)
        };
        let debug = Arc::new(Mutex::new(EmulatorDebug {
            override_fps: false,
            fps: FPS,
        }));

        let nes_state = Arc::new(Mutex::new(nes_state));
        let mut game_loop = GameLoop::new(nes_state.clone(), FPS);
        let mut loop_counter = RateCounter::new();
        let jh = tokio::spawn({
            let debug = debug.clone();
            async move {
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
        });
        Self {
            _jh: jh,
            nes_state,
            netplay_config: bundle.config.netplay.clone(),
            debug,
        }
    }

    pub fn save_state(&self) -> Option<Vec<u8>> {
        self.nes_state.lock().unwrap().save()
    }

    pub fn load_state(&mut self, data: &mut Vec<u8>) {
        self.nes_state.lock().unwrap().load(data);
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
impl GuiComponent<Emulator> for EmulatorGui {
    fn ui(&mut self, instance: &mut Emulator, ui: &mut egui::Ui, settings: &mut Settings) {
        self.netplay_gui
            .ui(&mut instance.nes_state.lock().unwrap(), ui, settings);

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

    fn messages(&self, instance: &Emulator) -> Option<Vec<String>> {
        self.netplay_gui
            .messages(&instance.nes_state.lock().unwrap())
    }

    fn name(&self) -> Option<String> {
        self.netplay_gui.name()
    }
}
