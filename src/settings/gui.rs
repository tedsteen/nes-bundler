use std::time::{Duration, Instant};

use egui::{
    load::SizedTexture, Align2, Color32, ColorImage, Context, Image, Order, RichText,
    TextureHandle, TextureOptions, Ui, Window,
};

use crate::{
    audio::{gui::AudioGui, Audio},
    input::{
        buttons::GamepadButton, gamepad::GamepadEvent, gui::InputsGui, keys::KeyCode, Inputs,
        KeyEvent,
    },
    integer_scaling::{calculate_size_corrected, Size},
    nes_state::{
        emulator::{Emulator, EmulatorGui},
        VideoFrame,
    },
    MINIMUM_INTEGER_SCALING_SIZE, NES_HEIGHT, NES_WIDTH, NES_WIDTH_4_3,
};

use super::Settings;
pub trait ToGuiEvent {
    /// Convert the struct to a GuiEvent
    fn to_gui_event(&self) -> Option<GuiEvent>;
}

#[derive(Clone, Debug)]
pub enum GuiEvent {
    Keyboard(KeyEvent),
    Gamepad(GamepadEvent),
}

pub trait GuiComponent<T> {
    fn ui(&mut self, instance: &mut T, ui: &mut Ui, settings: &mut Settings);

    //TODO: remove from gui component. Has nothing to do with a gui
    fn event(&mut self, _instance: &mut T, _event: &GuiEvent, _settings: &mut Settings) {}

    fn messages(&self, _instance: &T) -> Option<Vec<String>> {
        None
    }
    fn name(&self) -> Option<String> {
        None
    }
}

enum GuiWithState<'a> {
    Inputs(&'a mut InputsGui, &'a mut Inputs),
    Audio(&'a mut AudioGui, &'a mut Audio),
    Emulator(&'a mut EmulatorGui, &'a mut Emulator),
}

impl GuiWithState<'_> {
    fn ui(&mut self, ui: &mut Ui, settings: &mut Settings) {
        match self {
            GuiWithState::Inputs(gui, instance) => gui.ui(instance, ui, settings),
            GuiWithState::Audio(gui, instance) => gui.ui(instance, ui, settings),
            GuiWithState::Emulator(gui, instance) => gui.ui(instance, ui, settings),
        }
    }
    fn event(&mut self, event: &GuiEvent, settings: &mut Settings) {
        match self {
            GuiWithState::Inputs(gui, instance) => gui.event(instance, event, settings),
            GuiWithState::Audio(gui, instance) => gui.event(instance, event, settings),
            GuiWithState::Emulator(gui, instance) => gui.event(instance, event, settings),
        }
    }

    fn messages(&self) -> Option<Vec<String>> {
        match self {
            GuiWithState::Inputs(gui, instance) => gui.messages(instance),
            GuiWithState::Audio(gui, instance) => gui.messages(instance),
            GuiWithState::Emulator(gui, instance) => gui.messages(instance),
        }
    }

    fn name(&self) -> Option<String> {
        match self {
            GuiWithState::Inputs(gui, _) => gui.name(),
            GuiWithState::Audio(gui, _) => gui.name(),
            GuiWithState::Emulator(gui, _) => gui.name(),
        }
    }
}

pub struct Gui {
    inputs_gui: InputsGui,
    audio_gui: AudioGui,
    emulator_gui: EmulatorGui,

    start_time: Instant,
    visible: bool,
    pub nes_texture_handle: TextureHandle,
    nes_texture_options: TextureOptions,
}

impl Gui {
    pub fn new(ctx: &Context, emulator: &Emulator) -> Self {
        let nes_texture_options = TextureOptions {
            magnification: egui::TextureFilter::Nearest,
            minification: egui::TextureFilter::Nearest,
            wrap_mode: egui::TextureWrapMode::ClampToEdge,
        };
        Self {
            inputs_gui: InputsGui::new(),
            audio_gui: AudioGui {},
            emulator_gui: emulator.new_gui(),
            start_time: Instant::now(),
            visible: false,
            nes_texture_handle: ctx.load_texture(
                "nes",
                ColorImage::new([NES_WIDTH as usize, NES_HEIGHT as usize], Color32::BLACK),
                nes_texture_options,
            ),
            nes_texture_options,
        }
    }

    pub fn handle_event(
        &mut self,
        event: &GuiEvent,

        inputs: &mut Inputs,
        audio: &mut Audio,
        emulator: &mut Emulator,

        settings: &mut Settings,
    ) {
        match &event {
            GuiEvent::Gamepad(crate::input::gamepad::GamepadEvent::ButtonDown {
                button: GamepadButton::Guide,
                ..
            })
            | GuiEvent::Keyboard(KeyEvent::Pressed(KeyCode::Escape)) => {
                self.toggle_visibility();
            }
            _ => {
                let guis = &mut [
                    GuiWithState::Audio(&mut self.audio_gui, audio),
                    GuiWithState::Inputs(&mut self.inputs_gui, inputs),
                    GuiWithState::Emulator(&mut self.emulator_gui, emulator),
                ];
                for gui in guis {
                    gui.event(event, settings)
                }
            }
        }
    }

    pub fn ui(
        &mut self,
        ctx: &Context,

        inputs: &mut Inputs,
        audio: &mut Audio,
        emulator: &mut Emulator,

        settings: &mut Settings,
    ) {
        egui::Area::new("game_area")
            .fixed_pos([0.0, 0.0])
            .order(Order::Background)
            .show(ctx, |ui| {
                let texture_handle = &self.nes_texture_handle;
                if let Some(t) = ctx.tex_manager().read().meta(texture_handle.id()) {
                    if t.size[0] != 0 {
                        let available_size = ui.available_size();
                        let new_size = if available_size.x < MINIMUM_INTEGER_SCALING_SIZE.0 as f32
                            || available_size.y < MINIMUM_INTEGER_SCALING_SIZE.1 as f32
                        {
                            let width = NES_WIDTH_4_3;
                            let ratio_height = available_size.y / NES_HEIGHT as f32;
                            let ratio_width = available_size.x / width as f32;
                            let ratio = f32::min(ratio_height, ratio_width);
                            Size {
                                width: (width as f32 * ratio) as u32,
                                height: (NES_HEIGHT as f32 * ratio) as u32,
                            }
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
                            ui.add(Image::new(SizedTexture::new(
                                texture_handle,
                                (new_size.width as f32, new_size.height as f32),
                            )));
                        });
                    }
                }
            });
        let guis = &mut [
            GuiWithState::Audio(&mut self.audio_gui, audio),
            GuiWithState::Inputs(&mut self.inputs_gui, inputs),
            GuiWithState::Emulator(&mut self.emulator_gui, emulator),
        ];
        egui::Area::new("message_area")
            .fixed_pos([0.0, 0.0])
            .order(Order::Middle)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(50.0);

                    for gui in guis.iter() {
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
                for (idx, gui) in guis.iter_mut().enumerate() {
                    if let Some(name) = gui.name() {
                        if idx != 0 {
                            ui.separator();
                        }

                        ui.vertical_centered(|ui| {
                            ui.heading(name);
                        });

                        gui.ui(ui, settings);
                    }
                }
            });
    }

    pub fn toggle_visibility(&mut self) {
        self.visible = !self.visible;
    }

    pub(crate) fn update_nes_texture(&mut self, buffer: &VideoFrame) {
        self.nes_texture_handle.set(
            ColorImage::from_rgb([NES_WIDTH as usize, NES_HEIGHT as usize], buffer),
            self.nes_texture_options,
        );
    }
}
