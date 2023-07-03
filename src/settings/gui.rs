use egui::Context;
use winit::event::VirtualKeyCode;

use crate::GameRunner;

pub trait GuiComponent {
    fn ui(
        &mut self,
        ctx: &Context,
        game_runner: &mut GameRunner,
        ui_visible: bool,
        is_open: &mut bool,
    );
    fn name(&self) -> String;
}

pub struct SettingsContainer {
    open: bool,
    component: Box<dyn GuiComponent>,
}

impl SettingsContainer {
    pub fn new(component: Box<dyn GuiComponent>) -> Self {
        Self {
            open: false,
            component,
        }
    }
}

#[derive(Default)]
pub struct Gui {
    visible: bool,
    settings: Vec<SettingsContainer>,
}

impl Gui {
    pub fn add_settings(&mut self, component: Box<dyn GuiComponent>) {
        self.settings.push(SettingsContainer::new(component));
    }

    pub fn handle_event(&mut self, event: &winit::event::WindowEvent) {
        if let winit::event::WindowEvent::KeyboardInput { input, .. } = event {
            if let Some(code) = input.virtual_keycode {
                if input.state == winit::event::ElementState::Pressed
                    && code == VirtualKeyCode::Escape
                {
                    self.visible = !self.visible;
                }
            }
        }
    }

    pub fn ui(&mut self, ctx: &Context, game_runner: &mut GameRunner) {
        if self.visible {
            egui::TopBottomPanel::top("menubar_container").show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("Settings", |ui| {
                        for setting in &mut self.settings {
                            if ui.button(setting.component.name()).clicked() {
                                setting.open = !setting.open;
                                ui.close_menu();
                            }
                        }
                    })
                });
            });
        }

        for setting in &mut self.settings {
            setting
                .component
                .ui(ctx, game_runner, self.visible, &mut setting.open);
        }
    }
}
