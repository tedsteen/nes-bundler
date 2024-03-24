use std::sync::{Arc, Mutex};

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
    FPS,
};

use super::NesStateHandler;

pub struct Emulator {
    _jh: JoinHandle<()>,
    nes_state: Arc<Mutex<NetplayStateHandler>>,
    netplay_config: NetplayBuildConfiguration,
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
        let nes_state = Arc::new(Mutex::new(nes_state));
        let mut game_loop = GameLoop::new(nes_state.clone(), FPS);
        let mut loop_counter = RateCounter::new();
        let jh = tokio::spawn(async move {
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
                            game_loop.set_updates_per_second(frame_data.fps)
                        }
                    });
                });
                //sleep(Duration::from_millis(15)).await;
                tokio::task::yield_now().await
            }
        });
        Self {
            _jh: jh,
            nes_state,
            netplay_config: bundle.config.netplay.clone(),
        }
    }

    pub fn save_state(&self) -> Option<Vec<u8>> {
        self.nes_state.lock().unwrap().save()
    }

    pub fn load_state(&mut self, data: &mut Vec<u8>) {
        self.nes_state.lock().unwrap().load(data);
    }

    pub fn new_gui(&self) -> EmulatorGui {
        EmulatorGui::Netplay(NetplayGui::new(self.netplay_config.clone()))
    }
}

#[allow(dead_code)]
pub enum EmulatorGui {
    Local,
    Netplay(NetplayGui),
}
impl EmulatorGui {
    fn to_gui(&self) -> Option<&NetplayGui> {
        match self {
            EmulatorGui::Local => None,
            EmulatorGui::Netplay(gui) => Some(gui),
        }
    }
    fn to_gui_mut(&mut self) -> Option<&mut NetplayGui> {
        match self {
            EmulatorGui::Local => None,
            EmulatorGui::Netplay(gui) => Some(gui),
        }
    }
}
impl GuiComponent<Emulator> for EmulatorGui {
    fn ui(&mut self, instance: &mut Emulator, ui: &mut egui::Ui, settings: &mut Settings) {
        if let Some(gui) = self.to_gui_mut() {
            gui.ui(&mut instance.nes_state.lock().unwrap(), ui, settings)
        }
    }
    fn event(
        &mut self,
        instance: &mut Emulator,
        event: &crate::settings::gui::GuiEvent,
        settings: &mut Settings,
    ) {
        if let Some(gui) = self.to_gui_mut() {
            gui.event(&mut instance.nes_state.lock().unwrap(), event, settings);
        }
    }

    fn messages(&self, instance: &Emulator) -> Vec<String> {
        if let Some(gui) = self.to_gui() {
            gui.messages(&instance.nes_state.lock().unwrap())
        } else {
            [].to_vec()
        }
    }

    fn name(&self) -> Option<String> {
        if let Some(gui) = self.to_gui() {
            gui.name()
        } else {
            None
        }
    }
}
