use std::sync::Arc;

use crate::{
    emulation::{NES_HEIGHT, NES_WIDTH, VideoBufferPool},
    input::{KeyEvent, buttons::GamepadButton, gamepad::GamepadEvent, keys::Modifiers},
    window::{
        Fullscreen,
        egui_winit_wgpu::{Renderer, texture::Texture},
    },
};

use futures::executor::block_on;
use winit::window::Window;

use self::gui::{GuiEvent, MainGui, ToGuiEvent};
pub mod gui;

pub struct MainView {
    modifiers: Modifiers,
    nes_texture: Texture,
    renderer: Renderer,
    frame_buffer: VideoBufferPool,
    pub window: Arc<Window>,
}

fn to_egui_key(gamepad_button: &GamepadButton) -> Option<egui::Key> {
    match gamepad_button {
        GamepadButton::DPadUp => Some(egui::Key::ArrowUp),
        GamepadButton::DPadDown => Some(egui::Key::ArrowDown),
        GamepadButton::DPadLeft => Some(egui::Key::ArrowLeft),
        GamepadButton::DPadRight => Some(egui::Key::ArrowRight),
        GamepadButton::South => Some(egui::Key::Enter),
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
    pub fn new(window: Window, frame_buffer: VideoBufferPool) -> Self {
        let window = Arc::new(window);
        let mut renderer =
            block_on(Renderer::new(window.clone())).expect("a renderer to be created");
        Self {
            modifiers: Modifiers::empty(),

            nes_texture: Texture::new(&mut renderer, NES_WIDTH, NES_HEIGHT, Some("nes frame")),
            renderer,
            frame_buffer,
            window,
        }
    }

    pub fn handle_window_event(
        &mut self,
        window_event: &winit::event::WindowEvent,
        main_gui: &mut MainGui,
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
                self.handle_gui_event(winit_gui_event, main_gui);
            }
        }
    }

    pub fn handle_gui_event(&mut self, gui_event: &GuiEvent, main_gui: &mut MainGui) {
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
                        if main_gui.visible() {
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
            main_gui.handle_event(gui_event);
        }
    }

    pub fn render(&mut self, main_gui: &mut MainGui) {
        if let Some(nes_frame) = &self.frame_buffer.pop_ref() {
            self.nes_texture.update(&self.renderer.queue, nes_frame);
        }

        let nes_texture_id = self.nes_texture.get_id();
        let render_result = self.renderer.render(move |ctx| {
            main_gui.ui(ctx, nes_texture_id);
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
            Err(wgpu::SurfaceError::Other) => log::warn!("Other error"),
        };
    }
}
