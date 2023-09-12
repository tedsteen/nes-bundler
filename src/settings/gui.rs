use egui::{Context, Order, TextureHandle};

use crate::{
    input::{gamepad::GamepadEvent, KeyEvent},
    HEIGHT, WIDTH,
};
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
    fn ui(&mut self, ctx: &Context, ui_visible: bool, name: String);
    fn event(&mut self, event: &GuiEvent);
    fn name(&self) -> Option<String>;
    fn open(&mut self) -> &mut bool;
}

pub struct EmptyGuiComponent {
    is_open: bool,
}

impl EmptyGuiComponent {
    #[allow(dead_code)] //Only used when not using netplay
    pub fn new() -> Self {
        Self { is_open: false }
    }
}

impl GuiComponent for EmptyGuiComponent {
    fn ui(&mut self, _ctx: &Context, _ui_visible: bool, _name: String) {}
    fn name(&self) -> Option<String> {
        None
    }
    fn open(&mut self) -> &mut bool {
        &mut self.is_open
    }

    fn event(&mut self, _event: &GuiEvent) {}
}

pub struct Gui {
    visible: bool,
}

impl Gui {
    pub fn new(visible: bool) -> Self {
        Self { visible }
    }

    pub fn handle_events(&mut self, event: &GuiEvent, guis: Vec<&mut dyn GuiComponent>) {
        for gui in guis {
            gui.event(event);
        }
    }

    pub fn ui(
        &mut self,
        ctx: &Context,
        guis: &mut Vec<&mut dyn GuiComponent>,
        texture_handle: &TextureHandle,
    ) {
        egui::Area::new("game_area")
            .fixed_pos(egui::Pos2::new(0.0, 0.0))
            .order(Order::Background)
            .show(ctx, |ui| {
                if let Some(t) = ctx.tex_manager().read().meta(texture_handle.id()) {
                    if t.size[0] != 0 {
                        let texture_width = WIDTH as f32;
                        let texture_height = HEIGHT as f32;

                        let screen_width = ui.available_size().x;
                        let screen_height = ui.available_size().y;

                        let width_ratio = (screen_width / texture_width).max(1.0);
                        let height_ratio = (screen_height / texture_height).max(1.0);

                        // Get smallest scale size
                        let scale = width_ratio.clamp(1.0, height_ratio);

                        let scaled_width = texture_width * scale;
                        let scaled_height = texture_height * scale;
                        ui.centered_and_justified(|ui| {
                            ui.add(egui::Image::new(
                                texture_handle,
                                [scaled_width, scaled_height],
                            ));
                        });
                    }
                }
            });
        egui::Area::new("window_area")
            .fixed_pos(egui::Pos2::new(0.0, 0.0))
            .show(ctx, |ui| {
                if self.visible {
                    egui::TopBottomPanel::top("menubar_container").show_inside(ui, |ui| {
                        egui::menu::bar(ui, |ui| {
                            ui.menu_button("Settings", |ui| {
                                for gui in guis.iter_mut() {
                                    if let Some(name) = gui.name() {
                                        if ui.button(name).clicked() {
                                            *gui.open() = !*gui.open();
                                            ui.close_menu();
                                        };
                                    }
                                }
                            })
                        });
                    });
                }
                for gui in guis {
                    if let Some(name) = gui.name() {
                        gui.ui(ctx, self.visible, name);
                    }
                }
            });
    }

    pub fn toggle_visibility(&mut self) {
        self.visible = !self.visible;
    }
}
