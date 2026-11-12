use std::time::{Duration, Instant};

use egui::{
    Align, Align2, Color32, Context, FontId, Image, Label, Layout, Margin, Response, RichText,
    TextureId, Ui, UiBuilder, Vec2, Widget, load::SizedTexture,
};

use crate::{
    Size,
    audio::gui::AudioGui,
    emulation::{
        NES_HEIGHT, NES_WIDTH, NES_WIDTH_4_3, NesRegion, SharedEmulator, gui::EmulatorGui,
    },
    gui::{MenuButton, action_button, actions_ui, esc_pressed, section_ui, sizes},
    input::{JoypadState, KeyEvent, gamepad::GamepadEvent, gui::InputsGui},
    integer_scaling::{MINIMUM_INTEGER_SCALING_SIZE, calculate_size_corrected},
    settings::{MAX_PLAYERS, SettingsStore},
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
    // Runs when gui is visible. Returns a navigation action if the component wants to change menu state.
    fn ui(&mut self, ui: &mut Ui) -> Option<MainMenuState>;

    fn messages(&self, _menu_state: &MainMenuState) -> Option<Vec<String>> {
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
    emulator: SharedEmulator,
    audio_gui: AudioGui,
    pub inputs_gui: InputsGui,
    emulator_gui: EmulatorGui,
    supported_nes_regions: Vec<NesRegion>,
    settings: &'static SettingsStore,
    exit_requested: bool,
    menu_state: MainMenuState,
}

impl MainGui {
    // Convenience
    pub fn visible(&self) -> bool {
        !matches!(self.menu_state, MainMenuState::Closed)
    }

    /// Returns the current joypad states for all players, suppressed to zero while the menu
    /// is open so that UI navigation keys don't accidentally feed into the emulator.
    pub fn game_inputs(&self) -> [JoypadState; MAX_PLAYERS] {
        if self.visible() {
            [JoypadState(0); MAX_PLAYERS]
        } else {
            self.inputs_gui.inputs.joypads()
        }
    }

    const MESSAGE_TEXT_BACKGROUND: Color32 = Color32::from_rgba_premultiplied(20, 20, 20, 200);
    const MESSAGE_TEXT_COLOR: Color32 = Color32::from_rgb(255, 255, 255);

    pub fn new(
        emulator: SharedEmulator,
        audio_gui: AudioGui,
        inputs_gui: InputsGui,
        emulator_gui: EmulatorGui,
        supported_nes_regions: Vec<NesRegion>,
        settings: &'static SettingsStore,
    ) -> Self {
        Self {
            start_time: Instant::now(),
            emulator,
            audio_gui,
            inputs_gui,
            emulator_gui,
            supported_nes_regions,
            settings,
            exit_requested: false,
            menu_state: MainMenuState::Closed,
        }
    }

    pub fn take_exit_requested(&mut self) -> bool {
        std::mem::take(&mut self.exit_requested)
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
        let screen_rect = ctx.input(|a| a.content_rect());

        let window_title = title.unwrap_or("");
        egui::Window::new(window_title)
            .title_bar(title.is_some())
            .collapsible(false)
            .resizable(false)
            .movable(false)
            .frame(
                egui::Frame::window(&ctx.global_style()).inner_margin(Margin::same(sizes::MARGIN)),
            )
            .pivot(Align2::CENTER_CENTER)
            .fixed_pos(screen_rect.center())
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    egui::Grid::new(format!("main_menu_grid_{window_title}"))
                        .num_columns(1)
                        .spacing([sizes::ROW_SPACING, sizes::ROW_SPACING])
                        .show(ui, content);
                });
            });
    }

    pub const MENU_TINT: Color32 = Color32::from_rgb(50, 50, 50);

    fn for_each_component(&mut self, mut f: impl FnMut(&mut dyn GuiComponent)) {
        f(&mut self.audio_gui);
        f(&mut self.inputs_gui);
        f(&mut self.emulator_gui);
    }

    pub fn ui(&mut self, ui: &mut Ui, nes_texture_id: TextureId) {
        #[cfg(feature = "debug")]
        puffin::profile_scope!("ui");

        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("NES Frame");
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
                .show(ui, |ui| {
                    let available_size = ui.available_size();
                    let frame_size = if available_size.x < MINIMUM_INTEGER_SCALING_SIZE.width as f32
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
                                x: frame_size.width as f32,
                                y: frame_size.height as f32,
                            },
                        ));
                        if self.visible() {
                            nes_image = nes_image.tint(Self::MENU_TINT);
                        }
                        ui.add(nes_image);
                    });

                    // Messages are painted over the game, in the top of the screen.
                    let messages_rect = ui.max_rect().shrink(Self::MESSAGES_MARGIN);
                    ui.scope_builder(
                        UiBuilder::new()
                            .max_rect(messages_rect)
                            .layout(Layout::top_down(Align::Center)),
                        |ui| self.messages_ui(ui),
                    );
                });
        }
        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("Main ui");

            if !self.visible() && esc_pressed(ui) {
                self.menu_state = MainMenuState::Main;
            }
            match self.menu_state.clone() {
                MainMenuState::Main => {
                    Self::ui_main_container(None, ui, |ui| {
                        if Self::menu_item_ui(ui, "BACK").clicked() || esc_pressed(ui) {
                            self.menu_state = MainMenuState::Closed;
                        }

                        if let Some(name) = self.emulator_gui.name()
                            && Self::menu_item_ui(ui, name.to_uppercase()).clicked()
                        {
                            self.menu_state = MainMenuState::Netplay;
                        }

                        if Self::menu_item_ui(ui, "SETTINGS").clicked() {
                            self.menu_state = MainMenuState::Settings;
                        }

                        #[cfg(feature = "debug")]
                        {
                            if Self::menu_item_ui(ui, "PROFILING").clicked() {
                                puffin::set_scopes_on(!puffin::are_scopes_on());
                            }
                        }

                        if Self::menu_item_ui(ui, "QUIT GAME").clicked() {
                            self.exit_requested = true;
                        }
                    });
                }
                MainMenuState::Settings => {
                    Self::ui_main_container(Some("Settings"), ui, |ui| {
                        ui.vertical(|ui| {
                            if let Some(name) = self.audio_gui.name().map(str::to_owned) {
                                section_ui(ui, true, &name, |ui| {
                                    self.audio_gui.ui(ui);
                                });
                            }
                            if let Some(name) = self.inputs_gui.name().map(str::to_owned) {
                                section_ui(ui, false, &name, |ui| {
                                    self.inputs_gui.ui(ui);
                                });
                            }

                            if self.supported_nes_regions.len() > 1 {
                                section_ui(ui, false, "NES System", |ui| {
                                    ui.label(
                                        RichText::new("NOTE: changing this will restart the game")
                                            .color(Color32::DARK_RED),
                                    );
                                    ui.horizontal(|ui| {
                                        let mut settings = self.settings.write();
                                        for supported_region in &self.supported_nes_regions {
                                            if ui
                                                .radio_value(
                                                    settings.nes_region_mut(),
                                                    supported_region.clone(),
                                                    format!("{:?}", supported_region),
                                                )
                                                .changed()
                                            {
                                                self.emulator.request_reset(true);
                                            }
                                        }
                                    });
                                });
                            }
                        });
                        ui.end_row();

                        actions_ui(ui, |ui| {
                            if action_button("Close").ui(ui).clicked() || esc_pressed(ui) {
                                self.menu_state = MainMenuState::Main;
                            }
                        });
                    });
                }
                MainMenuState::Netplay => {
                    if let Some(name) = self.emulator_gui.name().map(str::to_owned) {
                        Self::ui_main_container(Some(&name), ui, |ui| {
                            if let Some(new_state) = self.emulator_gui.ui(ui) {
                                self.menu_state = new_state;
                            }
                        });
                    }
                }
                MainMenuState::Closed => {}
            }
        }
    }

    const MESSAGES_MARGIN: f32 = 80.0;

    fn messages_ui(&mut self, ui: &mut Ui) {
        let menu_state = self.menu_state.clone();
        self.for_each_component(|gui| {
            if let Some(messages) = gui.messages(&menu_state) {
                for message in messages {
                    Self::message_ui(ui, message);
                }
            }
        });
        if self.start_time.elapsed() < Duration::from_secs(5) {
            Self::message_ui(ui, "Press ESC for menu");
        }
    }

    pub fn handle_event(&mut self, gui_event: &GuiEvent) {
        self.for_each_component(|gui| gui.handle_event(gui_event));
    }
}
