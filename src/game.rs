use crate::input::buttons::GamepadButton;
use crate::nes_state::NesStateHandler;
use crate::{Fps, NES_HEIGHT, NES_WIDTH};

use base64::engine::general_purpose::STANDARD_NO_PAD as b64;
use base64::Engine;

use crate::input::keys::Modifiers;

use crate::audio::Audio;

use crate::input::Inputs;
use crate::nes_state::FrameData;
use crate::window::{Fullscreen, GlutinWindowContext};
use crate::{
    input::KeyEvent,
    settings::gui::{Gui, GuiEvent},
};

use egui::{ColorImage, ImageData};
use std::path::PathBuf;

use crate::settings::Settings;

pub struct Game {
    nes_state: Box<dyn NesStateHandler>,
    pub gui: Gui,
    pub settings: Settings,
    #[cfg(feature = "debug")]
    pub debug: crate::debug::Debug,
    audio: Audio,
    inputs: Inputs,
    modifiers: Modifiers,
    pub gl_window: GlutinWindowContext,
    pub settings_path: PathBuf,
}
impl Game {
    pub fn new(
        nes_state: Box<dyn NesStateHandler>,
        gui: Gui,
        settings: Settings,
        audio: Audio,
        inputs: Inputs,
        gl_window: GlutinWindowContext,
        settings_path: PathBuf,
    ) -> Self {
        Self {
            nes_state,
            gui,
            inputs,
            settings,
            #[cfg(feature = "debug")]
            debug: crate::debug::Debug::new(),
            audio,
            modifiers: Modifiers::empty(),
            gl_window,
            settings_path,
        }
    }
    pub fn apply_gui_event(&mut self, gui_event: &GuiEvent) {
        use crate::settings::gui::GuiEvent::Keyboard;
        if !match gui_event {
            Keyboard(KeyEvent::ModifiersChanged(modifiers)) => {
                self.modifiers = *modifiers;
                false
            }
            Keyboard(KeyEvent::Pressed(key_code)) => {
                let settings = &mut self.settings;
                let nes_state = &mut self.nes_state;

                use crate::input::keys::KeyCode::*;
                match key_code {
                    F1 => {
                        if let Some(save_state) = nes_state.save() {
                            settings.last_save_state = Some(b64.encode(save_state));
                            settings.save(&self.settings_path);
                        }
                        true
                    }
                    F2 => {
                        if let Some(save_state) = &settings.last_save_state {
                            if let Ok(buf) = &mut b64.decode(save_state) {
                                nes_state.load(buf);
                            }
                        }
                        true
                    }
                    Escape => {
                        self.gui.toggle_visibility();
                        true
                    }
                    key_code => self
                        .gl_window
                        .window_mut()
                        .check_and_set_fullscreen(&self.modifiers, key_code),
                }
            }
            GuiEvent::Gamepad(crate::input::gamepad::GamepadEvent::ButtonDown {
                button: GamepadButton::Guide,
                ..
            }) => {
                self.gui.toggle_visibility();
                true
            }
            _ => false,
        } {
            self.gui.handle_events(
                gui_event,
                &mut [
                    #[cfg(feature = "debug")]
                    Some(&mut self.debug),
                    Some(&mut self.inputs),
                    Some(&mut self.audio),
                    self.nes_state.get_gui(),
                ],
                &mut self.settings,
            )
        }
    }

    pub fn run_gui(&mut self) -> bool {
        let settings_hash_before = self.settings.get_hash();
        self.audio.sync_audio_devices(&mut self.settings.audio);

        self.gui.ui(
            self.gl_window.window(),
            &mut [
                #[cfg(feature = "debug")]
                Some(&mut self.debug),
                Some(&mut self.inputs),
                Some(&mut self.audio),
                self.nes_state.get_gui(),
            ],
            &mut self.settings,
        );
        settings_hash_before != self.settings.get_hash()
    }

    pub fn advance(&mut self) -> Option<FrameData> {
        self.nes_state
            .advance([self.inputs.get_joypad(0), self.inputs.get_joypad(1)])
    }

    pub fn draw_frame(&mut self, video_data: Option<&[u8]>) {
        let new_image_data = video_data.map(|video_data| {
            ImageData::Color(
                ColorImage::from_rgb([NES_WIDTH as usize, NES_HEIGHT as usize], video_data).into(),
            )
        });

        self.gui.update_nes_texture(new_image_data);
    }

    pub fn push_audio(&mut self, samples: &[i16], fps_hint: Fps) {
        self.audio.stream.push_samples(samples, fps_hint);
    }
}
