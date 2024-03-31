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
    settings::gui::{GuiEvent, SettingsGui},
    window::{
        egui_winit_wgpu::{texture::Texture, Renderer},
        Fullscreen, NESFrame,
    },
    MINIMUM_INTEGER_SCALING_SIZE, NES_HEIGHT, NES_WIDTH, NES_WIDTH_4_3,
};

pub struct MainGui {
    pub settings_gui: SettingsGui,
    pub emulator: Emulator,
    pub audio: Audio,
    pub inputs: Inputs,
    modifiers: Modifiers,

    nes_texture: Texture,
}
impl MainGui {
    pub fn new(renderer: &mut Renderer, emulator: Emulator, inputs: Inputs, audio: Audio) -> Self {
        Self {
            settings_gui: SettingsGui::new(),
            emulator,
            inputs,
            audio,
            modifiers: Modifiers::empty(),

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
                window.check_and_set_fullscreen(&self.modifiers, key_code)
            }
            _ => false,
        };
        if !consumed {
            self.inputs.advance(gui_event);
        }
    }

    pub fn render_gui(&mut self, renderer: &mut Renderer, nes_frame: &NESFrame) {
        self.nes_texture.update(&renderer.queue, nes_frame);
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
