use std::sync::{Arc, Mutex};

use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::{
    audio::AudioSender,
    bundle::Bundle,
    gameloop::GameLoop,
    input::JoypadState,
    settings::{gui::GuiComponent, Settings, MAX_PLAYERS},
    window::egui_winit_wgpu::VideoFramePool,
    FPS,
};

use super::NesStateHandler;

pub struct Emulator {
    jh: JoinHandle<()>,
    nes_state: Arc<Mutex<dyn NesStateHandler>>,
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
            crate::netplay::NetplayStateHandler::new(
                rom,
                netplay_rom,
                bundle.config.netplay.clone(),
                netplay_id,
            )
        };
        let nes_state = Arc::new(Mutex::new(nes_state));
        let mut game_loop = GameLoop::new(nes_state.clone(), FPS);

        let jh = tokio::spawn(async move {
            loop {
                //println!("LOOP");
                game_loop.next_frame(|game_loop| {
                    //println!("FRAME");

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
        Self { jh, nes_state }
    }

    pub fn save_state(&self) -> Option<Vec<u8>> {
        self.nes_state.lock().unwrap().save()
    }
    pub fn load_state(&mut self, data: &mut Vec<u8>) {
        self.nes_state.lock().unwrap().load(data);
    }
}
impl GuiComponent for Emulator {
    fn ui(&mut self, ui: &mut egui::Ui, settings: &mut Settings) {
        self.nes_state
            .lock()
            .unwrap()
            .get_gui()
            .unwrap()
            .ui(ui, settings);
    }

    fn messages(&self) -> Vec<String> {
        self.nes_state.lock().unwrap().get_gui().unwrap().messages()
    }

    fn event(&mut self, event: &crate::settings::gui::GuiEvent, settings: &mut Settings) {
        self.nes_state
            .lock()
            .unwrap()
            .get_gui()
            .unwrap()
            .event(event, settings);
    }

    fn name(&self) -> Option<String> {
        self.nes_state.lock().unwrap().get_gui().unwrap().name()
    }
}
