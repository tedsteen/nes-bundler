use std::time::Instant;

use egui::{load::SizedTexture, Context, Image, Order, Vec2};

use crate::{
    audio::Audio,
    input::{
        buttons::GamepadButton,
        keys::{KeyCode, Modifiers},
        Inputs, KeyEvent,
    },
    integer_scaling::{calculate_size_corrected, Size},
    nes_state::emulator::Emulator,
    settings::{
        gui::{GuiEvent, SettingsGui},
        Settings,
    },
    window::{
        egui_winit_wgpu::{texture::Texture, Renderer},
        Fullscreen, NESFramePool,
    },
    MINIMUM_INTEGER_SCALING_SIZE, NES_HEIGHT, NES_WIDTH, NES_WIDTH_4_3,
};

pub struct MainGui {
    last_render: Instant,
    settings_gui: SettingsGui,
    emulator: Emulator,
    audio: Audio,
    inputs: Inputs,
    modifiers: Modifiers,

    frame_pool: NESFramePool,
    nes_texture: Texture,
}
impl MainGui {
    pub fn new(
        renderer: &mut Renderer,
        frame_pool: NESFramePool,
        emulator: Emulator,
        inputs: Inputs,
        audio: Audio,
    ) -> Self {
        Self {
            last_render: Instant::now(),
            settings_gui: SettingsGui::new(),
            emulator,
            inputs,
            audio,
            modifiers: Modifiers::empty(),

            frame_pool,
            nes_texture: Texture::new(renderer, NES_WIDTH, NES_HEIGHT, Some("nes frame")),
        }
    }
}
impl MainGui {
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
                use crate::input::keys::KeyCode::*;
                use base64::engine::general_purpose::STANDARD_NO_PAD as b64;
                use base64::Engine;
                match key_code {
                    F1 => {
                        if let Some(save_state) = self.emulator.save_state() {
                            Settings::current().last_save_state = Some(b64.encode(save_state));
                        }
                        true
                    }
                    F2 => {
                        if let Some(save_state) = &Settings::current().last_save_state {
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
            self.inputs.advance(gui_event);
        }
    }

    pub fn render_gui(&mut self, renderer: &mut Renderer) {
        let now = Instant::now();
        // If emulation speed is low make sure to render the UI at least once every 20ms
        let mut needs_render = now.duration_since(self.last_render).as_millis() > 20;

        if let Some(frame_buffer) = self.frame_pool.pop_ref() {
            needs_render = true;
            self.nes_texture.update(&renderer.queue, &frame_buffer);
        }
        if needs_render {
            self.last_render = now;
            let render_result = renderer.render(move |ctx| {
                self.ui(ctx);
            });

            match render_result {
                Ok(_) => {}
                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                    // Reconfigure the surface if it's lost or outdated
                    log::warn!("Surface lost or outdated, recreating.");
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
    }

    fn ui(&mut self, ctx: &Context) {
        egui::Area::new("game_area")
            .fixed_pos([0.0, 0.0])
            .order(Order::Background)
            .show(ctx, |ui| {
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
                    ui.add(Image::from_texture(SizedTexture::new(
                        self.nes_texture.get_id(),
                        Vec2 {
                            x: new_size.width as f32,
                            y: new_size.height as f32,
                        },
                    )))
                });
            });

        self.settings_gui
            .ui(ctx, &mut self.audio, &mut self.inputs, &mut self.emulator);
    }
}
