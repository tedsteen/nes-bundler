use std::{
    sync::{OnceLock, RwLock},
    time::{Duration, Instant},
};

use egui::{
    Align2, Button, Color32, Context, FontId, Image, Label, Margin, Response, RichText, Style,
    TextureId, Ui, Vec2, Widget, load::SizedTexture,
};

use crate::{
    Size,
    audio::gui::AudioGui,
    bundle::Bundle,
    emulation::{
        EmulatorCommand, EmulatorCommandBus, NES_HEIGHT, NES_WIDTH, NES_WIDTH_4_3, gui::EmulatorGui,
    },
    gui::{MenuButton, esc_pressed},
    input::{KeyEvent, gamepad::GamepadEvent, gui::InputsGui},
    integer_scaling::{MINIMUM_INTEGER_SCALING_SIZE, calculate_size_corrected},
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
    fn ui(&mut self, ui: &mut Ui);

    fn messages(&self) -> Option<Vec<String>> {
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
    emulator_tx: EmulatorCommandBus,
    audio_gui: AudioGui,
    pub inputs_gui: InputsGui,
    emulator_gui: EmulatorGui,
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

    pub fn new(
        emulator_tx: EmulatorCommandBus,
        audio_gui: AudioGui,
        inputs_gui: InputsGui,
        emulator_gui: EmulatorGui,
    ) -> Self {
        Self {
            start_time: Instant::now(),
            emulator_tx,
            audio_gui,
            inputs_gui,
            emulator_gui,
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

    fn ui_main_container(title: Option<&str>, ctx: &Context, content: impl FnOnce(&mut Ui)) {
        let screen_rect = ctx.input(|a| a.screen_rect());

        let window_title = title.unwrap_or("");
        egui::Window::new(window_title)
            .title_bar(title.is_some())
            .collapsible(false)
            .resizable(false)
            .movable(false)
            .frame(egui::Frame::window(&Style::default()).inner_margin(Margin::same(20)))
            .pivot(Align2::CENTER_CENTER)
            .fixed_pos(screen_rect.center())
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    egui::Grid::new(format!("main_menu_grid_{window_title}"))
                        .num_columns(1)
                        .spacing([10.0, 10.0])
                        .show(ui, content);
                });
            });
    }

    pub const MENU_TINT: Color32 = Color32::from_rgb(50, 50, 50);

    pub fn ui(&mut self, ctx: &Context, nes_texture_id: TextureId) {
        #[cfg(feature = "debug")]
        puffin::profile_scope!("ui");

        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("NES Frame");
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
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
                        let mut nes_image = Image::from_texture(SizedTexture::new(
                            nes_texture_id,
                            Vec2 {
                                x: new_size.width as f32,
                                y: new_size.height as f32,
                            },
                        ));
                        if self.visible() {
                            nes_image = nes_image.tint(Self::MENU_TINT);
                        }
                        ui.add(nes_image);
                    });
                });
        }
        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("Main ui");

            if !self.visible() && esc_pressed(ctx) {
                Self::set_main_menu_state(MainMenuState::Main);
            }
            match Self::main_menu_state() {
                MainMenuState::Main => {
                    Self::ui_main_container(None, ctx, |ui| {
                        if Self::menu_item_ui(ui, "BACK").clicked() || esc_pressed(ctx) {
                            Self::set_main_menu_state(MainMenuState::Closed);
                        }

                        if let Some(name) = self.emulator_gui.name() {
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
                    Self::ui_main_container(Some("Settings"), ctx, |ui| {
                        ui.vertical(|ui| {
                            if let Some(name) = self.audio_gui.name() {
                                ui.vertical_centered(|ui| {
                                    ui.heading(name);
                                });
                                self.audio_gui.ui(ui);
                            }
                            ui.add_space(10.0);
                            ui.separator();
                            ui.add_space(10.0);
                            if let Some(name) = self.inputs_gui.name() {
                                ui.vertical_centered(|ui| {
                                    ui.heading(name);
                                });
                                self.inputs_gui.ui(ui);
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
                                                    .try_send(EmulatorCommand::Reset(true));
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
                    if self.emulator_gui.name().is_some() {
                        let name = self.emulator_gui.name().expect("a name").to_owned();
                        Self::ui_main_container(Some(&name), ctx, |ui| {
                            self.emulator_gui.ui(ui);
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
                        let gui_components: &mut [&mut dyn GuiComponent] = &mut [
                            &mut self.audio_gui,
                            &mut self.inputs_gui,
                            &mut self.emulator_gui,
                        ];
                        for gui in gui_components.iter_mut() {
                            if gui.name().is_some() {
                                {
                                    if let Some(messages) = gui.messages() {
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

    pub fn handle_event(&mut self, gui_event: &GuiEvent) {
        let gui_components: &mut [&mut dyn GuiComponent] = &mut [
            &mut self.audio_gui,
            &mut self.inputs_gui,
            &mut self.emulator_gui,
        ];

        for gui in gui_components {
            gui.handle_event(gui_event);
        }
    }
}
