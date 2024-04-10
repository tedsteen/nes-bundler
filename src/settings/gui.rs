use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use egui::{
    Align2, Color32, Context, CursorIcon, FontId, Id, Margin, Response, RichText, Sense, TextStyle,
    Ui, Vec2, Widget, WidgetInfo, WidgetText, WidgetType, Window,
};
use winit::dpi::LogicalSize;

use crate::{
    audio::gui::AudioGui,
    emulation::gui::EmulatorGui,
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

pub struct MainMenuButton {
    text: WidgetText,
    sense: Sense,
}
impl MainMenuButton {
    const ACTIVE_COLOR: Color32 = Color32::WHITE;
    const UNACTIVE_COLOR: Color32 = Color32::from_rgb(96, 96, 96);

    fn new(text: impl Into<String>) -> Self {
        Self {
            text: RichText::new(text)
                .color(Color32::PLACEHOLDER)
                .strong()
                .font(FontId::monospace(30.0))
                .into(),
            sense: Sense::click(),
        }
    }
}
impl Widget for MainMenuButton {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        let mut desired_size = Vec2::ZERO;
        let galley =
            self.text
                .into_galley(ui, Some(false), ui.available_width(), TextStyle::Button);

        desired_size.x += galley.size().x;
        desired_size.y = desired_size.y.max(galley.size().y);
        let (rect, mut response) = ui.allocate_at_least(desired_size, self.sense);
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, galley.text()));
        if ui.is_rect_visible(rect) {
            let text_pos = ui.layout().align_size_within_rect(galley.size(), rect).min;
            response = response.on_hover_cursor(CursorIcon::PointingHand);
            if response.hovered() {
                ui.memory_mut(|m| {
                    m.request_focus(Id::NULL);
                });
            }
            ui.painter().galley(
                text_pos,
                galley,
                if response.has_focus() || response.hovered() {
                    Self::ACTIVE_COLOR
                } else {
                    Self::UNACTIVE_COLOR
                },
            );
        }
        response
    }
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
    last_focused_id: Option<Id>,
    back_id: Option<Id>,
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
            last_focused_id: None,
            back_id: None,
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
        let res = ui.vertical_centered(|ui| MainMenuButton::new(text).ui(ui));
        ui.end_row();
        res.inner
    }

    fn main_window(
        window: &Arc<winit::window::Window>,
        ctx: &Context,
        title: Option<&str>,
        content: impl FnOnce(&mut Ui),
    ) {
        let size: LogicalSize<f32> = window.inner_size().to_logical(window.scale_factor());

        Window::new(title.unwrap_or(""))
            .title_bar(title.is_some())
            .collapsible(false)
            .resizable(false)
            .movable(false)
            .pivot(Align2::CENTER_CENTER)
            .fixed_pos([size.width / 2.0, size.height / 2.0])
            .show(ctx, content);
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
                    Self::main_window(&self.window, ctx, None, |ui| {
                        ui.add_space(20.0);
                        egui::Grid::new("main_menu_grid")
                            .num_columns(1)
                            .spacing([10.0, 10.0])
                            .show(ui, |ui| {
                                let back = Self::menu_item_ui(ui, "BACK");
                                self.back_id = Some(back.id); //Save for later...

                                ui.memory_mut(|m| {
                                    // This means that we want to clear the focus (a mouse has been moved over the menu items)
                                    if let Some(Id::NULL) = m.focus() {
                                        self.last_focused_id = None;
                                        m.surrender_focus(Id::NULL);
                                    } else if m.focus().is_none() {
                                        if let Some(last_focused_id) = self.last_focused_id {
                                            m.request_focus(last_focused_id);
                                        }
                                    } else {
                                        self.last_focused_id = m.focus();
                                    }
                                });

                                if back.clicked() {
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
                    Self::main_window(&self.window, ctx, Some("Settings"), |ui| {
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
                        Self::main_window(&self.window, ctx, Some(&name), |ui| {
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
        gui_components: &mut [&mut dyn GuiComponent],
    ) {
        // Make sure we focus something if a key is pressed to ensure that we can navigate
        if self.last_focused_id.is_none() {
            self.last_focused_id = self.back_id;
        }
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
                            self.state = MainMenuState::Main;
                        }
                    }
                }
            }
            _ => {
                for gui in gui_components {
                    gui.handle_event(gui_event);
                }
            }
        }
    }
}
