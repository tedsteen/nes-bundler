use std::time::{Duration, Instant};

use egui::{Align2, Color32, Context, Order, RichText, Ui, Window};

use crate::{
    input::{gamepad::GamepadEvent, KeyEvent},
    MINIMUM_INTEGER_SCALING_SIZE,
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
    // Runs every frame
    fn prepare(&mut self) {}

    // Runs if gui is visible
    fn ui(&mut self, _ui: &mut Ui) {}

    fn messages(&self) -> Option<Vec<String>> {
        None
    }
    fn name(&self) -> Option<String> {
        None
    }
    fn handle_event(&mut self, _gui_event: &GuiEvent) {}
}

pub struct SettingsGui {
    gui_components: Vec<Box<dyn GuiComponent>>,

    start_time: Instant,
    visible: bool,
}

impl SettingsGui {
    pub fn new(gui_components: Vec<Box<dyn GuiComponent>>) -> Self {
        Self {
            gui_components,
            start_time: Instant::now(),
            visible: false,
        }
    }

    pub fn ui(&mut self, ctx: &Context) {
        egui::Area::new("message_area")
            .fixed_pos([0.0, 0.0])
            .order(Order::Middle)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(50.0);

                    for gui in self.gui_components.iter_mut() {
                        gui.prepare();
                        if gui.name().is_some() {
                            if let Some(messages) = gui.messages() {
                                for message in messages {
                                    ui.heading(message);
                                }
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
                for (idx, gui) in self.gui_components.iter_mut().enumerate() {
                    if let Some(name) = gui.name() {
                        if idx != 0 {
                            ui.separator();
                        }

                        ui.vertical_centered(|ui| {
                            ui.heading(name);
                        });

                        gui.ui(ui);
                    }
                }
                #[cfg(feature = "debug")]
                {
                    ui.separator();
                    let mut profile = puffin::are_scopes_on();
                    ui.checkbox(&mut profile, "Toggle profiling");
                    puffin::set_scopes_on(profile);
                }
            });
    }

    pub fn toggle_visibility(&mut self) {
        self.visible = !self.visible;
    }

    pub fn handle_event(&mut self, gui_event: &GuiEvent) {
        for gui in &mut self.gui_components {
            gui.handle_event(gui_event);
        }
    }
}
