use std::time::{Duration, Instant};

use egui::{
    epaint::ImageDelta, load::SizedTexture, Align2, Color32, ColorImage, Image, ImageData, Order,
    RichText, TextureHandle, TextureOptions, Ui, Window,
};

use crate::{
    input::{gamepad::GamepadEvent, KeyEvent},
    integer_scaling::{calculate_size_corrected, Size},
    MINIMUM_INTEGER_SCALING_SIZE, NES_HEIGHT, NES_WIDTH, NES_WIDTH_4_3,
};

use super::Settings;
pub trait ToGuiEvent {
    /// Convert the struct to a GuiEvent
    fn to_gui_event(&self) -> Option<GuiEvent>;
}

#[derive(Clone, Debug)]
pub enum GuiEvent {
    Keyboard(KeyEvent),
    Gamepad(GamepadEvent),
}
pub trait GuiComponent {
    fn ui(&mut self, ui: &mut Ui, settings: &mut Settings);
    fn messages(&self) -> Vec<String>;
    fn event(&mut self, event: &GuiEvent, settings: &mut Settings);
    fn name(&self) -> Option<String>;
    fn open(&mut self) -> &mut bool;
}

pub struct Gui {
    start_time: Instant,
    visible: bool,
    egui_glow: egui_glow::EguiGlow,
    nes_texture: TextureHandle,
    nes_texture_options: TextureOptions,
    no_image: ImageData,
}

impl Gui {
    pub fn new(egui_glow: egui_glow::EguiGlow) -> Self {
        let nes_texture_options = TextureOptions {
            magnification: egui::TextureFilter::Nearest,
            minification: egui::TextureFilter::Nearest,
            wrap_mode: egui::TextureWrapMode::ClampToEdge,
        };

        Self {
            start_time: Instant::now(),
            visible: false,
            nes_texture: egui_glow.egui_ctx.load_texture(
                "nes",
                ImageData::Color(
                    ColorImage::new([NES_WIDTH as usize, NES_HEIGHT as usize], Color32::BLACK)
                        .into(),
                ),
                nes_texture_options,
            ),
            egui_glow,
            nes_texture_options,
            no_image: ImageData::Color(ColorImage::new([0, 0], Color32::TRANSPARENT).into()),
        }
    }

    pub fn handle_events(
        &mut self,
        event: &GuiEvent,
        guis: &mut [Option<&mut dyn GuiComponent>],
        settings: &mut Settings,
    ) {
        for gui in guis.iter_mut().flatten() {
            gui.event(event, settings);
        }
    }

    pub fn ui(
        &mut self,
        window: &winit::window::Window,
        guis: &mut [Option<&mut dyn GuiComponent>],
        settings: &mut Settings,
    ) {
        self.egui_glow.run(window, |ctx| {
            egui::Area::new("game_area")
                .fixed_pos([0.0, 0.0])
                .order(Order::Background)
                .show(ctx, |ui| {
                    let texture_handle = &self.nes_texture;
                    if let Some(t) = ctx.tex_manager().read().meta(texture_handle.id()) {
                        if t.size[0] != 0 {
                            let available_size = ui.available_size();
                            let new_size = if available_size.x
                                < MINIMUM_INTEGER_SCALING_SIZE.0 as f32
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
            egui::Area::new("message_area")
                .fixed_pos([0.0, 0.0])
                .order(Order::Middle)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(50.0);
                        for gui in guis.iter_mut().flatten() {
                            if gui.name().is_some() {
                                for message in gui.messages() {
                                    ui.heading(message);
                                }
                            }
                        }
                        if self.start_time.elapsed() < Duration::new(5, 0) {
                            ui.heading(
                                RichText::new("Press ESC to see settings")
                                    .heading()
                                    .strong()
                                    .background_color(Color32::from_rgba_premultiplied(
                                        20, 20, 20, 180,
                                    )),
                            );
                        }
                    });
                });

            Window::new("Settings")
                .open(&mut self.visible)
                .collapsible(false)
                .resizable(false)
                .movable(true)
                .pivot(Align2::CENTER_CENTER)
                .default_pos([
                    MINIMUM_INTEGER_SCALING_SIZE.0 as f32 / 2.0,
                    MINIMUM_INTEGER_SCALING_SIZE.1 as f32 / 2.0,
                ])
                .show(ctx, |ui| {
                    for (idx, gui) in guis.iter_mut().flatten().enumerate() {
                        if let Some(name) = gui.name() {
                            if idx != 0 {
                                ui.separator();
                            }

                            ui.vertical_centered(|ui| {
                                ui.heading(name);
                            });

                            gui.ui(ui, settings);
                        }
                    }
                });
        });
    }

    pub fn toggle_visibility(&mut self) {
        self.visible = !self.visible;
    }

    pub(crate) fn update_nes_texture(&self, new_image_data: Option<ImageData>) {
        self.egui_glow.egui_ctx.tex_manager().write().set(
            self.nes_texture.id(),
            ImageDelta::full(
                new_image_data.unwrap_or_else(|| self.no_image.clone()),
                self.nes_texture_options,
            ),
        );
    }

    pub(crate) fn on_event(
        &mut self,
        window: &winit::window::Window,
        event: &winit::event::WindowEvent,
    ) -> bool {
        self.egui_glow.on_window_event(window, event).consumed
    }
    pub fn destroy(&mut self) {
        self.egui_glow.destroy();
    }

    pub fn paint(&mut self, window: &winit::window::Window) {
        self.egui_glow.paint(window);
    }
}
