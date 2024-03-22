use crate::nes_state::emulator::Emulator;
use crate::window::egui_winit_wgpu::State;
use crate::window::Fullscreen;

use crate::input::keys::Modifiers;

use crate::audio::Audio;

use crate::input::Inputs;

use crate::{
    input::KeyEvent,
    settings::gui::{Gui, GuiEvent},
};

use std::path::PathBuf;

use crate::settings::Settings;

pub struct Game {
    pub emulator: Emulator,
    pub settings: Settings,
    #[cfg(feature = "debug")]
    pub debug: crate::debug::Debug,
    pub audio: Audio,
    pub inputs: Inputs,
    modifiers: Modifiers,
    pub settings_path: PathBuf,
}
impl Game {
    pub fn new(
        emulator: Emulator,
        settings: Settings,
        audio: Audio,
        inputs: Inputs,
        settings_path: PathBuf,
    ) -> Self {
        Self {
            emulator,
            inputs,
            settings,
            #[cfg(feature = "debug")]
            debug: crate::debug::Debug::new(),
            audio,
            modifiers: Modifiers::empty(),
            settings_path,
        }
    }
    pub fn handle_event(
        &mut self,
        gui_event: &GuiEvent,
        gui: &mut Gui,
        window: &winit::window::Window,
    ) {
        use crate::settings::gui::GuiEvent::Keyboard;
        let consumed = match gui_event {
            Keyboard(KeyEvent::ModifiersChanged(modifiers)) => {
                self.modifiers = *modifiers;
                false
            }
            Keyboard(KeyEvent::Pressed(key_code)) => {
                let settings = &mut self.settings;
                //let nes_state = &mut self.nes_state;

                use crate::input::keys::KeyCode::*;
                use base64::engine::general_purpose::STANDARD_NO_PAD as b64;
                use base64::Engine;
                match key_code {
                    F1 => {
                        if let Some(save_state) = self.emulator.save_state() {
                            settings.last_save_state = Some(b64.encode(save_state));
                            settings.save(&self.settings_path);
                        }
                        true
                    }
                    F2 => {
                        if let Some(save_state) = &settings.last_save_state {
                            if let Ok(buf) = &mut b64.decode(save_state) {
                                self.emulator.load_state(buf);
                            }
                        }
                        true
                    }
                    key_code => window.check_and_set_fullscreen(&self.modifiers, key_code),
                }
            }
            _ => false,
        };
        if !consumed {
            gui.handle_event(
                gui_event,
                &mut [
                    #[cfg(feature = "debug")]
                    Some(&mut self.debug),
                    Some(&mut self.inputs),
                    Some(&mut self.audio),
                    Some(&mut self.emulator),
                ],
                &mut self.settings,
            )
        }
    }

    pub fn render_gui(&mut self, state: &mut State, gui: &mut Gui) {
        if let Some(frame_buffer) = state.frame_pool.pop_ref() {
            gui.update_nes_texture(&frame_buffer);
        }
        let render_result = state.render(move |ctx| {
            self.audio.sync_audio_devices(&mut self.settings.audio);
            let settings_hash_before = self.settings.get_hash();
            gui.ui(
                ctx,
                &mut [
                    #[cfg(feature = "debug")]
                    Some(&mut self.debug),
                    Some(&mut self.inputs),
                    Some(&mut self.audio),
                    Some(&mut self.emulator),
                ],
                &mut self.settings,
            );
            if settings_hash_before != self.settings.get_hash() {
                self.settings.save(&self.settings_path);
            }
        });

        match render_result {
            Ok(_) => {}
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                // Reconfigure the surface if it's lost or outdated

                state.resize(state.size);
            }
            // The system is out of memory, we should probably quit
            Err(wgpu::SurfaceError::OutOfMemory) => {
                log::warn!("Out of memory when rendering")
                // control_flow.exit(),
            }
            Err(wgpu::SurfaceError::Timeout) => log::warn!("Surface timeout"),
        };
    }
}
