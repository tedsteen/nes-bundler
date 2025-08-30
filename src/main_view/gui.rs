use std::{
    sync::{Arc, OnceLock, RwLock},
    time::{Duration, Instant},
};

use egui::{
    Align2, Button, Color32, Context, FontId, Label, Margin, Response, RichText, Style, Ui, Widget,
};
use winit::dpi::LogicalSize;

use crate::{
    audio::gui::AudioGui,
    bundle::Bundle,
    emulation::{Emulator, EmulatorCommand, gui::EmulatorGui},
    gui::{MenuButton, esc_pressed},
    input::{KeyEvent, gamepad::GamepadEvent, gui::InputsGui},
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
    // Runs when gui is visible
    fn ui(&mut self, ui: &mut Ui, emulator: &mut Emulator);

    fn messages(&self, _emulator: &Emulator) -> Option<Vec<String>> {
        None
    }
    fn name(&self) -> Option<&str> {
        None
    }
    fn handle_event(&mut self, _gui_event: &GuiEvent) {}
}

#[derive(Debug, Clone)]
pub enum MainMenuState {
    Closed,

    Main,
    Settings,
    Netplay,
}
pub struct MainGui {
    start_time: Instant,
    window: Arc<winit::window::Window>,
}

impl MainGui {
    fn _main_menu_state() -> &'static RwLock<MainMenuState> {
        static MEM: OnceLock<RwLock<MainMenuState>> = OnceLock::new();
        MEM.get_or_init(|| RwLock::new(MainMenuState::Closed))
    }
    pub fn set_main_menu_state(main_menu_state: MainMenuState) {
        *Self::_main_menu_state().write().unwrap() = main_menu_state;
    }
    pub fn main_menu_state() -> MainMenuState {
        Self::_main_menu_state().read().unwrap().clone()
    }

    // Convenience
    pub fn visible(&self) -> bool {
        !matches!(Self::main_menu_state(), MainMenuState::Closed)
    }

    const MESSAGE_TEXT_BACKGROUND: Color32 = Color32::from_rgba_premultiplied(20, 20, 20, 200);
    const MESSAGE_TEXT_COLOR: Color32 = Color32::from_rgb(255, 255, 255);

    pub fn new(window: Arc<winit::window::Window>) -> Self {
        Self {
            start_time: Instant::now(),
            window,
        }
    }

    fn message_ui(ui: &mut Ui, text: impl Into<String>) {
        ui.add(
            Label::new(
                RichText::new(text)
                    .font(FontId::monospace(30.0))
                    .strong()
                    .background_color(Self::MESSAGE_TEXT_BACKGROUND)
                    .color(Self::MESSAGE_TEXT_COLOR),
            )
            .selectable(false),
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
            .frame(egui::Frame::window(&Style::default()).inner_margin(Margin::same(20)))
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
        emulator: &mut Emulator,
    ) {
        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("Main ui");

            if !self.visible() && esc_pressed(ctx) {
                Self::set_main_menu_state(MainMenuState::Main);
            }
            match Self::main_menu_state() {
                MainMenuState::Main => {
                    Self::ui_main_container(&self.window, None, ctx, |ui| {
                        if Self::menu_item_ui(ui, "BACK").clicked() || esc_pressed(ctx) {
                            Self::set_main_menu_state(MainMenuState::Closed);
                        }

                        if let Some(name) = emulator_gui.name() {
                            if Self::menu_item_ui(ui, name.to_uppercase()).clicked() {
                                Self::set_main_menu_state(MainMenuState::Netplay);
                            }
                        }

                        if Self::menu_item_ui(ui, "SETTINGS").clicked() {
                            Self::set_main_menu_state(MainMenuState::Settings);
                        }

                        #[cfg(feature = "debug")]
                        {
                            if Self::menu_item_ui(ui, "PROFILING").clicked() {
                                puffin::set_scopes_on(!puffin::are_scopes_on());
                            }
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
                                audio_gui.ui(ui, emulator);
                            }
                            ui.add_space(10.0);
                            ui.separator();
                            ui.add_space(10.0);
                            if let Some(name) = inputs_gui.name() {
                                ui.vertical_centered(|ui| {
                                    ui.heading(name);
                                });
                                inputs_gui.ui(ui, emulator);
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
                                                let _ = emulator
                                                    .command_tx
                                                    .send(EmulatorCommand::Reset(true));
                                            }
                                        }
                                    });
                                });
                            }

                            ui.vertical_centered(|ui| {
                                ui.add_space(20.0);
                                if Button::new(
                                    RichText::new("Close").font(FontId::proportional(20.0)),
                                )
                                .ui(ui)
                                .clicked()
                                    || esc_pressed(ui.ctx())
                                {
                                    Self::set_main_menu_state(MainMenuState::Main);
                                }
                            });
                        });
                    });
                }
                MainMenuState::Netplay => {
                    if emulator_gui.name().is_some() {
                        let name = emulator_gui.name().expect("a name").to_owned();
                        Self::ui_main_container(&self.window, Some(&name), ctx, |ui| {
                            emulator_gui.ui(ui, emulator);
                        });
                    }
                }
                MainMenuState::Closed => {}
            }
        }
        {
            egui::TopBottomPanel::top("messages")
                .show_separator_line(false)
                .frame(
                    egui::Frame::default()
                        .fill(Color32::TRANSPARENT)
                        .outer_margin(Margin::same(80))
                        .inner_margin(Margin::ZERO),
                )
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        let gui_components: &mut [&mut dyn GuiComponent] =
                            &mut [audio_gui, inputs_gui, emulator_gui];
                        for gui in gui_components.iter_mut() {
                            if gui.name().is_some() {
                                {
                                    if let Some(messages) = gui.messages(emulator) {
                                        for message in messages {
                                            Self::message_ui(ui, message);
                                        }
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
    }

    pub fn handle_event(
        &mut self,
        gui_event: &GuiEvent,
        audio_gui: &mut AudioGui,
        inputs_gui: &mut InputsGui,
        emulator_gui: &mut EmulatorGui,
    ) {
        let gui_components: &mut [&mut dyn GuiComponent] =
            &mut [audio_gui, inputs_gui, emulator_gui];

        for gui in gui_components {
            gui.handle_event(gui_event);
        }
    }
}
