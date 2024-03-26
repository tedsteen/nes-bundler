use std::sync::{Arc, Mutex};

use crate::bundle::Bundle;
use crate::window::NESFramePool;
use crate::{
    audio::Audio,
    fps::RateCounter,
    input::{Inputs, JoypadState},
    settings::{gui::GuiComponent, MAX_PLAYERS},
};
use anyhow::Result;

use crate::nes_state::NesStateHandler;

pub struct Emulator {
    pub frame_pool: NESFramePool,
    #[cfg(feature = "netplay")]
    nes_state: Arc<Mutex<crate::netplay::NetplayStateHandler>>,
    #[cfg(not(feature = "netplay"))]
    nes_state: Arc<Mutex<crate::nes_state::LocalNesState>>,
    joypads: Arc<Mutex<[JoypadState; MAX_PLAYERS]>>,
}
impl Emulator {
    pub fn start(inputs: &Inputs, audio: &mut Audio) -> Result<Self> {
        #[cfg(not(feature = "netplay"))]
        let nes_state = crate::nes_state::LocalNesState::load_rom(&Bundle::current().rom);

        #[cfg(feature = "netplay")]
        let nes_state = { crate::netplay::NetplayStateHandler::new() };

        let audio_tx = audio.stream.start()?;

        let frame_pool = NESFramePool::new();
        let shared_state = Emulator {
            frame_pool: frame_pool.clone(),
            nes_state: Arc::new(Mutex::new(nes_state)),
            joypads: inputs.joypads.clone(),
        };

        tokio::spawn({
            let nes_state = shared_state.nes_state.clone();
            let joypads = shared_state.joypads.clone();
            async move {
                let mut loop_counter = RateCounter::new();
                loop {
                    loop_counter.tick("Frames");

                    let joypads = *joypads.lock().unwrap();
                    let mut frame_data = match frame_pool.push_with(|video_frame| {
                        nes_state
                            .lock()
                            .unwrap()
                            .advance([joypads[0], joypads[1]], &mut Some(video_frame))
                    }) {
                        Ok(frame_data) => frame_data,
                        Err(_) => {
                            loop_counter.tick("Dropped Frames");
                            nes_state
                                .lock()
                                .unwrap()
                                .advance([joypads[0], joypads[1]], &mut None)
                        }
                    };

                    if let Some(frame_data) = &mut frame_data {
                        log::trace!("Pushing {:} audio samples", frame_data.audio.len());
                        for &s in &frame_data.audio {
                            let _ = audio_tx.send(s).await;
                        }
                    } else {
                        log::trace!("No frame, pushing silence");
                        let _ = audio_tx.send(0.0).await;
                    }

                    if let Some(report) = loop_counter.report() {
                        log::debug!("{report}");
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
            netplay_gui: crate::netplay::gui::NetplayGui::new(
                Bundle::current().config.netplay.clone(),
            ),
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
