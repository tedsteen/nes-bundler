use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use egui::{Color32, Context, FontId, Margin, Response, RichText, Ui, Widget};

use crate::{
    audio::gui::AudioGui,
    emulation::gui::EmulatorGui,
    gui::{centered_window, MenuButton},
    input::{
        buttons::GamepadButton, gamepad::GamepadEvent, gui::InputsGui, keys::KeyCode, KeyEvent,
    },
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

#[derive(Debug)]
enum MainMenuState {
    Main,
    Settings,
    Netplay,
}
pub struct SettingsGui {
    start_time: Instant,
    pub visible: bool,
    state: MainMenuState,
    window: Arc<winit::window::Window>,
}

impl SettingsGui {
    const MESSAGE_TEXT_BACKGROUND: Color32 = Color32::from_rgba_premultiplied(20, 20, 20, 200);
    const MESSAGE_TEXT_COLOR: Color32 = Color32::from_rgb(255, 255, 255);

    pub fn new(window: Arc<winit::window::Window>) -> Self {
        Self {
            start_time: Instant::now(),
            visible: false,
            state: MainMenuState::Main,
            window,
        }
    }
    fn message_ui(ui: &mut Ui, text: impl Into<String>) {
        ui.label(
            RichText::new(text)
                .font(FontId::monospace(30.0))
                .strong()
                .background_color(Self::MESSAGE_TEXT_BACKGROUND)
                .color(Self::MESSAGE_TEXT_COLOR),
        );
    }

    fn menu_item_ui(ui: &mut Ui, text: impl Into<String>) -> Response {
        let res = ui.vertical_centered(|ui| MenuButton::new(text).ui(ui));
        ui.end_row();
        res.inner
    }

    pub fn ui(
        &mut self,
        ctx: &Context,
        audio_gui: &mut AudioGui,
        inputs_gui: &mut InputsGui,
        emulator_gui: &mut EmulatorGui,
    ) {
        if self.visible {
            match &mut self.state {
                MainMenuState::Main => {
                    centered_window(&self.window, ctx, None, |ui| {
                        ui.add_space(20.0);
                        egui::Grid::new("main_menu_grid")
                            .num_columns(1)
                            .spacing([10.0, 10.0])
                            .show(ui, |ui| {
                                if Self::menu_item_ui(ui, "BACK").clicked() {
                                    self.visible = false;
                                }

                                if let Some(name) = emulator_gui.name() {
                                    if Self::menu_item_ui(ui, name.to_uppercase()).clicked() {
                                        self.state = MainMenuState::Netplay;
                                    }
                                }

                                if Self::menu_item_ui(ui, "SETTINGS").clicked() {
                                    self.state = MainMenuState::Settings;
                                }

                                if Self::menu_item_ui(ui, "EXIT").clicked() {
                                    std::process::exit(0);
                                }
                            });
                        ui.add_space(20.0);
                    });
                }
                MainMenuState::Settings => {
                    centered_window(&self.window, ctx, Some("Settings"), |ui| {
                        if let Some(name) = audio_gui.name() {
                            ui.vertical_centered(|ui| {
                                ui.heading(name);
                            });
                            audio_gui.ui(ui);
                        }
                        ui.separator();
                        if let Some(name) = inputs_gui.name() {
                            ui.vertical_centered(|ui| {
                                ui.heading(name);
                            });
                            inputs_gui.ui(ui);
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
                MainMenuState::Netplay => {
                    if let Some(name) = emulator_gui.name() {
                        centered_window(&self.window, ctx, Some(&name), |ui| {
                            emulator_gui.ui(ui);
                        });
                    }
                }
            }
        }

        egui::TopBottomPanel::top("messages")
            .show_separator_line(false)
            .frame(
                egui::Frame::default()
                    .fill(Color32::TRANSPARENT)
                    .outer_margin(Margin::same(80.0))
                    .inner_margin(Margin::ZERO),
            )
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    let gui_components: &mut [&mut dyn GuiComponent] =
                        &mut [audio_gui, inputs_gui, emulator_gui];
                    for gui in gui_components.iter_mut() {
                        gui.prepare();
                        if gui.name().is_some() {
                            if let Some(messages) = gui.messages() {
                                for message in messages {
                                    Self::message_ui(ui, message);
                                }
                            }
                        }
                    }
                    if self.start_time.elapsed() < Duration::from_secs(5) {
                        Self::message_ui(ui, "Press ESC for menu");
                    }
                });
            });
    }

    pub fn handle_event(
        &mut self,
        gui_event: &GuiEvent,
        audio_gui: &mut AudioGui,
        inputs_gui: &mut InputsGui,
        emulator_gui: &mut EmulatorGui,
    ) {
        match gui_event {
            GuiEvent::Gamepad(crate::input::gamepad::GamepadEvent::ButtonDown {
                button: GamepadButton::Guide,
                ..
            })
            | GuiEvent::Keyboard(KeyEvent::Pressed(KeyCode::Escape)) => {
                if !self.visible {
                    self.visible = true;
                } else {
                    match self.state {
                        MainMenuState::Main => {
                            self.visible = false;
                        }
                        MainMenuState::Settings | MainMenuState::Netplay => {
                            //TODO: check if the emulator_gui is modal and refuse to change state in that case?
                            self.state = MainMenuState::Main;
                        }
                    }
                }
            }
            _ => {
                let gui_components: &mut [&mut dyn GuiComponent] =
                    &mut [audio_gui, inputs_gui, emulator_gui];

                for gui in gui_components {
                    gui.handle_event(gui_event);
                }
            }
        }
    }
}
