use egui::Context;
use winit::event::{Event, VirtualKeyCode, WindowEvent};

pub trait GuiComponent {
    fn ui(&mut self, ctx: &Context, ui_visible: bool, name: String);
    fn event(&mut self, event: &winit::event::Event<()>);
    fn name(&self) -> Option<String>;
    fn open(&mut self) -> &mut bool;
}

pub struct EmptyGuiComponent {
    is_open: bool,
}

impl GuiComponent for EmptyGuiComponent {
    fn ui(&mut self, _ctx: &egui::Context, _ui_visible: bool, _name: String) {}
    fn name(&self) -> Option<String> {
        None
    }
    fn open(&mut self) -> &mut bool {
        &mut self.is_open
    }

    fn event(&mut self, _event: &winit::event::Event<()>) {}
}

#[derive(Default)]
pub struct Gui {
    visible: bool,
}

impl Gui {
    pub fn handle_event(
        &mut self,
        event: &winit::event::Event<()>,
        guis: Vec<&mut dyn GuiComponent>,
    ) {
        if let Event::WindowEvent {
            event: WindowEvent::KeyboardInput { input, .. },
            ..
        } = event
        {
            if let Some(code) = input.virtual_keycode {
                if input.state == winit::event::ElementState::Pressed
                    && code == VirtualKeyCode::Escape
                {
                    self.visible = !self.visible;
                }
            }
        }
        for gui in guis {
            gui.event(event);
        }
    }

    pub fn ui(&mut self, ctx: &Context, guis: &mut Vec<&mut dyn GuiComponent>) {
        if self.visible {
            egui::TopBottomPanel::top("menubar_container").show(ctx, |ui| {
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
    }
}
