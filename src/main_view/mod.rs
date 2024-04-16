use std::sync::mpsc::Sender;

use egui::{load::SizedTexture, Color32, Image, Vec2};

use crate::{
    audio::gui::AudioGui,
    emulation::{
        gui::EmulatorGui, BufferPool, EmulatorCommand, NES_HEIGHT, NES_WIDTH, NES_WIDTH_4_3,
    },
    input::{
        buttons::GamepadButton, gamepad::GamepadEvent, gui::InputsGui, keys::Modifiers, KeyEvent,
    },
    integer_scaling::{calculate_size_corrected, MINIMUM_INTEGER_SCALING_SIZE},
    window::{
        egui_winit_wgpu::{texture::Texture, Renderer},
        Fullscreen,
    },
    Size,
};

use self::gui::{GuiEvent, MainGui, ToGuiEvent};
pub mod gui;

pub struct MainView {
    pub main_gui: MainGui,
    modifiers: Modifiers,
    nes_texture: Texture,
    renderer: Renderer,
}

fn to_egui_key(gamepad_button: &GamepadButton) -> Option<egui::Key> {
    match gamepad_button {
        GamepadButton::DPadUp => Some(egui::Key::ArrowUp),
        GamepadButton::DPadDown => Some(egui::Key::ArrowDown),
        GamepadButton::DPadLeft => Some(egui::Key::ArrowLeft),
        GamepadButton::DPadRight => Some(egui::Key::ArrowRight),
        GamepadButton::A => Some(egui::Key::Enter),
        GamepadButton::Guide => Some(egui::Key::Escape),
        _ => None,
    }
}

fn to_egui_key_event(key: egui::Key, pressed: bool) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

fn to_egui_event(gamepad_event: &GamepadEvent) -> Option<egui::Event> {
    match gamepad_event {
        GamepadEvent::ButtonDown { button, .. } => {
            to_egui_key(button).map(|e| to_egui_key_event(e, true))
        }
        GamepadEvent::ButtonUp { button, .. } => {
            to_egui_key(button).map(|e| to_egui_key_event(e, false))
        }
        _ => None,
    }
}

impl MainView {
    pub fn new(mut renderer: Renderer, emulator_tx: Sender<EmulatorCommand>) -> Self {
        Self {
            main_gui: MainGui::new(renderer.window.clone(), emulator_tx),
            modifiers: Modifiers::empty(),

            nes_texture: Texture::new(&mut renderer, NES_WIDTH, NES_HEIGHT, Some("nes frame")),
            renderer,
        }
    }

    pub fn handle_window_event(
        &mut self,
        window_event: &winit::event::WindowEvent,
        audio_gui: &mut AudioGui,
        inputs_gui: &mut InputsGui,
        emulator_gui: &mut EmulatorGui,
    ) {
        if let winit::event::WindowEvent::Resized(physical_size) = window_event {
            self.renderer.resize(*physical_size);
        }

        if !self
            .renderer
            .egui
            .handle_input(&self.renderer.window, window_event)
            .consumed
        {
            if let Some(winit_gui_event) = &window_event.to_gui_event() {
                self.handle_gui_event(winit_gui_event, audio_gui, inputs_gui, emulator_gui);
            }
        }
    }

    pub fn handle_gui_event(
        &mut self,
        gui_event: &GuiEvent,
        audio_gui: &mut AudioGui,
        inputs_gui: &mut InputsGui,
        emulator_gui: &mut EmulatorGui,
    ) {
        use gui::GuiEvent::Keyboard;

        let consumed = match gui_event {
            Keyboard(KeyEvent::ModifiersChanged(modifiers)) => {
                self.modifiers = *modifiers;
                false
            }
            Keyboard(KeyEvent::Pressed(key_code)) => self
                .renderer
                .window
                .check_and_set_fullscreen(self.modifiers, *key_code),
            _ => {
                if let GuiEvent::Gamepad(gamepad_event) = gui_event {
                    if let Some(event) = to_egui_event(gamepad_event) {
                        if self.main_gui.visible() {
                            // If the gui is visible convert gamepad events to fake input events so we can control the ui with the gamepad
                            self.renderer.egui.state.egui_input_mut().events.push(event)
                        } else {
                            // If the gui is not visible pass on only the guide button
                            if matches!(
                                gamepad_event,
                                GamepadEvent::ButtonDown {
                                    button: GamepadButton::Guide,
                                    ..
                                } | GamepadEvent::ButtonUp {
                                    button: GamepadButton::Guide,
                                    ..
                                }
                            ) {
                                self.renderer.egui.state.egui_input_mut().events.push(event)
                            }
                        }
                    }
                }

                false
            }
        };
        if !consumed {
            self.main_gui
                .handle_event(gui_event, audio_gui, inputs_gui, emulator_gui);
        }
    }

    pub const MENU_TINT: Color32 = Color32::from_rgb(50, 50, 50);

    pub fn render(
        &mut self,
        frame_buffer: &BufferPool,
        audio_gui: &mut AudioGui,
        inputs_gui: &mut InputsGui,
        emulator_gui: &mut EmulatorGui,
    ) {
        #[cfg(feature = "debug")]
        puffin::profile_function!();

        if let Some(nes_frame) = &frame_buffer.pop_ref() {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("update nes texture");

            self.nes_texture.update(&self.renderer.queue, nes_frame);
        }

        let nes_texture_id = self.nes_texture.get_id();
        let main_gui = &mut self.main_gui;
        let render_result = self.renderer.render(move |ctx| {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("ui");

            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(egui::Color32::BLACK))
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
                        if main_gui.visible() {
                            nes_image = nes_image.tint(Self::MENU_TINT);
                        }
                        ui.add(nes_image);
                    });
                });
            main_gui.ui(ctx, audio_gui, inputs_gui, emulator_gui);
        });

        match render_result {
            Ok(_) => {}
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                // Reconfigure the surface if it's lost or outdated
                log::warn!("Surface lost or outdated, recreating.");
                self.renderer.resize(self.renderer.size);
            }
            // The system is out of memory, we should probably quit
            Err(wgpu::SurfaceError::OutOfMemory) => {
                log::warn!("Out of memory when rendering")
                // control_flow.exit(),
            }
            Err(wgpu::SurfaceError::Timeout) => log::warn!("Surface timeout"),
        };
    }
}
