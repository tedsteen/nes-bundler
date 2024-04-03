use std::{ops::Deref, sync::Arc};

use egui::{load::SizedTexture, Context, Image, Order, Vec2};
use thingbuf::ThingBuf;

use crate::{
    emulation::{FrameRecycle, NESVideoFrame, NES_HEIGHT, NES_WIDTH, NES_WIDTH_4_3},
    input::{
        buttons::GamepadButton,
        keys::{KeyCode, Modifiers},
        KeyEvent,
    },
    integer_scaling::{calculate_size_corrected, MINIMUM_INTEGER_SCALING_SIZE},
    settings::gui::{GuiComponent, GuiEvent, SettingsGui},
    window::{
        egui_winit_wgpu::{texture::Texture, Renderer},
        Fullscreen,
    },
    Size,
};

pub struct MainView {
    pub settings_gui: SettingsGui,
    modifiers: Modifiers,
    nes_texture: Texture,
    pub frame_pool: BufferPool,
}
impl MainView {
    pub fn new(renderer: &mut Renderer, gui_components: Vec<Box<dyn GuiComponent>>) -> Self {
        Self {
            settings_gui: SettingsGui::new(gui_components),
            modifiers: Modifiers::empty(),

            nes_texture: Texture::new(renderer, NES_WIDTH, NES_HEIGHT, Some("nes frame")),
            frame_pool: BufferPool::new(),
        }
    }
}
impl MainView {
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
                window.check_and_set_fullscreen(self.modifiers, *key_code)
            }
            _ => false,
        };
        if !consumed {
            self.settings_gui.handle_event(gui_event);
        }
    }

    pub fn render(&mut self, renderer: &mut Renderer) {
        if let Some(video) = self.frame_pool.pop_ref() {
            self.nes_texture.update(&renderer.queue, &video);
        }

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
                let new_size = if available_size.x < MINIMUM_INTEGER_SCALING_SIZE.width as f32
                    || available_size.y < MINIMUM_INTEGER_SCALING_SIZE.height as f32
                {
                    let width = NES_WIDTH_4_3;
                    let ratio_height = available_size.y / NES_HEIGHT as f32;
                    let ratio_width = available_size.x / width as f32;
                    let ratio = f32::min(ratio_height, ratio_width);
                    Size::new(
                        (width as f32 * ratio) as u32,
                        (NES_HEIGHT as f32 * ratio) as u32,
                    )
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

        self.settings_gui.ui(ctx);
    }
}

#[derive(Debug)]
pub struct BufferPool(Arc<ThingBuf<NESVideoFrame, FrameRecycle>>);

impl BufferPool {
    pub fn new() -> Self {
        Self(Arc::new(ThingBuf::with_recycle(1, FrameRecycle)))
    }
}

impl Default for BufferPool {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for BufferPool {
    type Target = Arc<ThingBuf<NESVideoFrame, FrameRecycle>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Clone for BufferPool {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}
