use std::path::PathBuf;

use egui::{
    load::SizedTexture, Color32, ColorImage, Context, Image, Order, TextureHandle, TextureOptions,
};

use crate::{
    audio::Audio,
    input::{
        buttons::GamepadButton,
        keys::{KeyCode, Modifiers},
        Inputs, KeyEvent,
    },
    integer_scaling::{calculate_size_corrected, Size},
    nes_state::{
        emulator::{Emulator, EmulatorGui},
        VideoFrame,
    },
    settings::{
        gui::{GuiEvent, SettingsGui},
        Settings,
    },
    window::{egui_winit_wgpu::Renderer, Fullscreen},
    MINIMUM_INTEGER_SCALING_SIZE, NES_HEIGHT, NES_WIDTH, NES_WIDTH_4_3,
};

pub struct MainGui {
    pub settings_gui: SettingsGui,
    pub nes_texture_handle: TextureHandle,
    nes_texture_options: TextureOptions,

    pub emulator: Emulator,
    pub settings: Settings,
    pub audio: Audio,
    pub inputs: Inputs,
    modifiers: Modifiers,
    pub settings_path: PathBuf,
}
impl MainGui {
    pub fn new(
        ctx: &Context,
        emulator_gui: EmulatorGui,
        emulator: Emulator,
        settings: Settings,
        audio: Audio,
        inputs: Inputs,
        settings_path: PathBuf,
    ) -> Self {
        let nes_texture_options = TextureOptions {
            magnification: egui::TextureFilter::Nearest,
            minification: egui::TextureFilter::Nearest,
            wrap_mode: egui::TextureWrapMode::ClampToEdge,
        };

        Self {
            settings_gui: SettingsGui::new(emulator_gui),
            nes_texture_handle: ctx.load_texture(
                "nes",
                ColorImage::new([NES_WIDTH as usize, NES_HEIGHT as usize], Color32::BLACK),
                nes_texture_options,
            ),
            nes_texture_options,
            emulator,
            inputs,
            settings,
            audio,
            modifiers: Modifiers::empty(),
            settings_path,
        }
    }
}
impl MainGui {
    pub(crate) fn update_nes_texture(&mut self, buffer: &VideoFrame) {
        self.nes_texture_handle.set(
            ColorImage::from_rgb([NES_WIDTH as usize, NES_HEIGHT as usize], buffer),
            self.nes_texture_options,
        );
    }
    pub fn handle_event(&mut self, gui_event: &GuiEvent, window: &winit::window::Window) {
        use crate::settings::gui::GuiEvent::Keyboard;

        let consumed = match gui_event {
            Keyboard(KeyEvent::ModifiersChanged(modifiers)) => {
                self.modifiers = *modifiers;
                false
            }
            GuiEvent::Gamepad(crate::input::gamepad::GamepadEvent::ButtonDown {
                button: GamepadButton::Guide,
                ..
            })
            | GuiEvent::Keyboard(KeyEvent::Pressed(KeyCode::Escape)) => {
                self.settings_gui.toggle_visibility();
                true
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
            self.inputs.advance(gui_event, &mut self.settings);
        }
    }

    pub fn render_gui(&mut self, renderer: &mut Renderer) {
        if let Some(frame_buffer) = renderer.frame_pool.pop_ref() {
            self.update_nes_texture(&frame_buffer);
        }
        let render_result = renderer.render(move |ctx| {
            self.audio.sync_audio_devices(&mut self.settings.audio);
            let settings_hash_before = self.settings.get_hash();
            self.ui(ctx);
            if settings_hash_before != self.settings.get_hash() {
                self.settings.save(&self.settings_path);
            }
        });

        match render_result {
            Ok(_) => {}
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                // Reconfigure the surface if it's lost or outdated

                renderer.resize(renderer.size);
            }
            // The system is out of memory, we should probably quit
            Err(wgpu::SurfaceError::OutOfMemory) => {
                log::warn!("Out of memory when rendering")
                // control_flow.exit(),
            }
            Err(wgpu::SurfaceError::Timeout) => log::warn!("Surface timeout"),
        };
    }

    fn ui(&mut self, ctx: &Context) {
        egui::Area::new("game_area")
            .fixed_pos([0.0, 0.0])
            .order(Order::Background)
            .show(ctx, |ui| {
                let texture_handle = &self.nes_texture_handle;
                if let Some(t) = ctx.tex_manager().read().meta(texture_handle.id()) {
                    if t.size[0] != 0 {
                        let available_size = ui.available_size();
                        let new_size = if available_size.x < MINIMUM_INTEGER_SCALING_SIZE.0 as f32
                            || available_size.y < MINIMUM_INTEGER_SCALING_SIZE.1 as f32
                        {
                            let width = NES_WIDTH_4_3;
                            let ratio_height = available_size.y / NES_HEIGHT as f32;
                            let ratio_width = available_size.x / width as f32;
                            let ratio = f32::min(ratio_height, ratio_width);
                            Size {
                                width: (width as f32 * ratio) as u32,
                                height: (NES_HEIGHT as f32 * ratio) as u32,
                            }
                        } else {
                            calculate_size_corrected(
                                available_size.x as u32,
                                available_size.y as u32,
                                NES_WIDTH,
                                NES_HEIGHT,
                                4.0,
                                3.0,
                            )
                        };

                        ui.centered_and_justified(|ui| {
                            ui.add(Image::new(SizedTexture::new(
                                texture_handle,
                                (new_size.width as f32, new_size.height as f32),
                            )));
                        });
                    }
                }
            });

        self.settings_gui.ui(
            ctx,
            &mut self.inputs,
            &mut self.audio,
            &mut self.emulator,
            &mut self.settings,
        );
    }
}
