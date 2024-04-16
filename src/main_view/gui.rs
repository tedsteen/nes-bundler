use std::{
    sync::{mpsc::Sender, Arc, OnceLock, RwLock, RwLockWriteGuard},
    time::{Duration, Instant},
};

use egui::{
    Align2, Button, Color32, Context, FontId, Margin, Response, RichText, Style, Ui, Widget,
};
use winit::dpi::LogicalSize;

use crate::{
    audio::gui::AudioGui,
    bundle::Bundle,
    emulation::{gui::EmulatorGui, EmulatorCommand},
    gui::MenuButton,
    input::{
        buttons::GamepadButton, gamepad::GamepadEvent, gui::InputsGui, keys::KeyCode, KeyEvent,
    },
    settings::Settings,
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

    fn messages(&self, _main_menu_state: &MainMenuState) -> Option<Vec<String>> {
        None
    }
    fn name(&self) -> Option<&str> {
        None
    }
    fn handle_event(&mut self, _gui_event: &GuiEvent) {}
}

#[derive(Debug)]
pub enum MainMenuState {
    Main,
    Settings,
    Netplay,
}
pub struct MainGui {
    start_time: Instant,
    state: MainMenuState,
    window: Arc<winit::window::Window>,
    emulator_tx: Sender<EmulatorCommand>,
}

impl MainGui {
    fn main_menu_visible<'a>() -> RwLockWriteGuard<'a, bool> {
        //TODO: Look into AtomicBool
        static MEM: OnceLock<RwLock<bool>> = OnceLock::new();
        MEM.get_or_init(|| RwLock::new(false)).write().unwrap()
    }

    // Convenience
    pub fn visible(&self) -> bool {
        *MainGui::main_menu_visible()
    }

    pub fn set_main_menu_visibility(visible: bool) {
        *Self::main_menu_visible() = visible;
    }

    const MESSAGE_TEXT_BACKGROUND: Color32 = Color32::from_rgba_premultiplied(20, 20, 20, 200);
    const MESSAGE_TEXT_COLOR: Color32 = Color32::from_rgb(255, 255, 255);

    pub fn new(window: Arc<winit::window::Window>, emulator_tx: Sender<EmulatorCommand>) -> Self {
        Self {
            start_time: Instant::now(),
            window,
            state: MainMenuState::Main,
            emulator_tx,
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

    fn ui_main_container(
        window: &Arc<winit::window::Window>,
        title: Option<&str>,
        ctx: &Context,
        content: impl FnOnce(&mut Ui),
    ) {
        let size: LogicalSize<f32> = window.inner_size().to_logical(window.scale_factor());
        let window_title = title.unwrap_or("");
        egui::Window::new(window_title)
            .title_bar(title.is_some())
            .collapsible(false)
            .resizable(false)
            .movable(false)
            .frame(egui::Frame::window(&Style::default()).inner_margin(Margin::same(20.0)))
            .pivot(Align2::CENTER_CENTER)
            .fixed_pos([size.width / 2.0, size.height / 2.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    egui::Grid::new(format!("main_menu_grid_{window_title}"))
                        .num_columns(1)
                        .spacing([10.0, 10.0])
                        .show(ui, content);
                });
            });
    }
    pub fn ui(
        &mut self,
        ctx: &Context,
        audio_gui: &mut AudioGui,
        inputs_gui: &mut InputsGui,
        emulator_gui: &mut EmulatorGui,
    ) {
        if self.visible() {
            match self.state {
                MainMenuState::Main => {
                    Self::ui_main_container(&self.window, None, ctx, |ui| {
                        if Self::menu_item_ui(ui, "BACK").clicked() {
                            Self::set_main_menu_visibility(false);
                        }

                        if let Some(name) = emulator_gui.name() {
                            if Self::menu_item_ui(ui, name.to_uppercase()).clicked() {
                                self.state = MainMenuState::Netplay;
                            }
                        }

                        if Self::menu_item_ui(ui, "SETTINGS").clicked() {
                            self.state = MainMenuState::Settings;
                        }

                        if Self::menu_item_ui(ui, "QUIT GAME").clicked() {
                            std::process::exit(0);
                        }
                    });
                }
                MainMenuState::Settings => {
                    Self::ui_main_container(&self.window, Some("Settings"), ctx, |ui| {
                        ui.vertical(|ui| {
                            if let Some(name) = audio_gui.name() {
                                ui.vertical_centered(|ui| {
                                    ui.heading(name);
                                });
                                audio_gui.ui(ui);
                            }
                            ui.add_space(10.0);
                            ui.separator();
                            ui.add_space(10.0);
                            if let Some(name) = inputs_gui.name() {
                                ui.vertical_centered(|ui| {
                                    ui.heading(name);
                                });
                                inputs_gui.ui(ui);
                            }

                            if Bundle::current().config.supported_nes_regions.len() > 1 {
                                ui.separator();
                                ui.vertical_centered(|ui| {
                                    ui.heading("NES System");
                                });
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new("NOTE: changing this will restart the game")
                                            .color(Color32::DARK_RED),
                                    );

                                    ui.horizontal(|ui| {
                                        for supported_region in
                                            &Bundle::current().config.supported_nes_regions
                                        {
                                            if ui
                                                .radio_value(
                                                    Settings::current_mut().get_nes_region(),
                                                    supported_region.clone(),
                                                    format!("{:?}", supported_region),
                                                )
                                                .changed()
                                            {
                                                let _ = self
                                                    .emulator_tx
                                                    .send(EmulatorCommand::Reset(true));
                                            }
                                        }
                                    });
                                });
                            }

                            #[cfg(feature = "debug")]
                            {
                                ui.add_space(10.0);
                                ui.separator();
                                ui.add_space(10.0);

                                let mut profile = puffin::are_scopes_on();
                                ui.checkbox(&mut profile, "Toggle profiling");
                                puffin::set_scopes_on(profile);
                            }

                            ui.vertical_centered(|ui| {
                                ui.add_space(20.0);
                                if Button::new(
                                    RichText::new("Close").font(FontId::proportional(20.0)),
                                )
                                .ui(ui)
                                .clicked()
                                {
                                    MainGui::set_main_menu_visibility(false);
                                }
                            });
                        });
                    });
                }
                MainMenuState::Netplay => {
                    if emulator_gui.name().is_some() {
                        let name = emulator_gui.name().expect("a name").to_owned();
                        Self::ui_main_container(&self.window, Some(&name), ctx, |ui| {
                            emulator_gui.ui(ui);
                        });
                    }
                }
            }
        } else {
            // Always go to main state if hidden
            self.state = MainMenuState::Main;
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
                            if let Some(messages) = gui.messages(&self.state) {
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
                if !self.visible() {
                    Self::set_main_menu_visibility(true);
                } else {
                    match self.state {
                        MainMenuState::Main => {
                            Self::set_main_menu_visibility(false);
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
