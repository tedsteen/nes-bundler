use egui::{ClippedMesh, FontDefinitions};
use egui_wgpu_backend::{BackendError, RenderPass, ScreenDescriptor};
use egui_winit_platform::{Platform, PlatformDescriptor};
use pixels::{wgpu, PixelsContext};
use std::time::Instant;
use winit::{event::VirtualKeyCode, window::Window};
use winit::event::VirtualKeyCode::*;
use crate::joypad_mappings::JoypadMappings;

/// Manages all state required for rendering egui over `Pixels`.
pub(crate) struct Gui {
    // State for egui.
    start_time: Instant,
    platform: Platform,
    screen_descriptor: ScreenDescriptor,
    rpass: RenderPass,
    paint_jobs: Vec<ClippedMesh>,

    // State for the demo app.
    pub show_gui: bool,
    pub latency: u16
}

const AVAILABLE_KEY_CODES: &'static [VirtualKeyCode] = &[Key1, Key2, Key3, Key4, Key5, Key6, Key7, Key8, Key9, Key0, A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z, Insert, Home, Delete, End, PageDown, PageUp, Left, Up, Right, Down, Back, Return, Space, Numlock, Numpad0, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9, NumpadAdd, NumpadDivide, NumpadDecimal, NumpadComma, NumpadEnter, NumpadEquals, NumpadMultiply, NumpadSubtract, LAlt, LControl, LShift, LWin, RAlt, RControl, RShift, RWin, Tab];

impl Gui {
    /// Create egui.
    pub(crate) fn new(width: u32, height: u32, scale_factor: f64, pixels: &pixels::Pixels) -> Self {
        let platform = Platform::new(PlatformDescriptor {
            physical_width: width,
            physical_height: height,
            scale_factor,
            font_definitions: FontDefinitions::default(),
            style: Default::default(),
        });
        let screen_descriptor = ScreenDescriptor {
            physical_width: width,
            physical_height: height,
            scale_factor: scale_factor as f32,
        };
        let rpass = RenderPass::new(pixels.device(), pixels.render_texture_format(), 1);

        Self {
            start_time: Instant::now(),
            platform,
            screen_descriptor,
            rpass,
            paint_jobs: Vec::new(),
            show_gui: false,
            latency: 100
        }
    }

    /// Handle input events from the window manager.
    pub(crate) fn handle_event(&mut self, event: &winit::event::Event<'_, ()>) {
        self.platform.handle_event(event);
    }

    /// Resize egui.
    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.screen_descriptor.physical_width = width;
            self.screen_descriptor.physical_height = height;
        }
    }

    /// Update scaling factor.
    pub(crate) fn scale_factor(&mut self, scale_factor: f64) {
        self.screen_descriptor.scale_factor = scale_factor as f32;
    }

    /// Prepare egui.
    pub(crate) fn prepare(&mut self, window: &Window, pad1: &mut JoypadMappings, pad2: &mut JoypadMappings) {
        self.platform
            .update_time(self.start_time.elapsed().as_secs_f64());

        // Begin the egui frame.
        self.platform.begin_frame();

        // Draw the demo application.
        self.ui(&self.platform.context(), pad1, pad2);

        // End the egui frame and create all paint jobs to prepare for rendering.
        let (_output, paint_commands) = self.platform.end_frame(Some(window));
        self.paint_jobs = self.platform.context().tessellate(paint_commands);
    }
    
    /// Create the UI using egui.
    fn ui(&mut self, ctx: &egui::CtxRef, pad1: &mut JoypadMappings, pad2: &mut JoypadMappings) {
        if self.show_gui {
            egui::Window::new("Joypad Mappings").collapsible(false).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Audio latency");
                    ui.add(egui::Slider::new(&mut self.latency, 1..=500).suffix("ms"));
                });
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label("Joypad #1");
                        ui.group(|ui| {
                            self.make_button_combo(ui, "Pad 1 - Up", &mut pad1.up);
                            self.make_button_combo(ui, "Pad 1 - Down", &mut pad1.down);
                            self.make_button_combo(ui, "Pad 1 - Left", &mut pad1.left);
                            self.make_button_combo(ui, "Pad 1 - Right", &mut pad1.right);
                            self.make_button_combo(ui, "Pad 1 - Start", &mut pad1.start);
                            self.make_button_combo(ui, "Pad 1 - Select", &mut pad1.select);
                            self.make_button_combo(ui, "Pad 1 - B", &mut pad1.b);
                            self.make_button_combo(ui, "Pad 1 - A", &mut pad1.a);
                        });
                    });

                    ui.vertical(|ui| {
                        ui.label("Joypad #2");
                        ui.group(|ui| {
                            self.make_button_combo(ui, "Pad 2 - Up", &mut pad2.up);
                            self.make_button_combo(ui, "Pad 2 - Down", &mut pad2.down);
                            self.make_button_combo(ui, "Pad 2 - Left", &mut pad2.left);
                            self.make_button_combo(ui, "Pad 2 - Right", &mut pad2.right);
                            self.make_button_combo(ui, "Pad 2 - Start", &mut pad2.start);
                            self.make_button_combo(ui, "Pad 2 - Select", &mut pad2.select);
                            self.make_button_combo(ui, "Pad 2 - B", &mut pad2.b);
                            self.make_button_combo(ui, "Pad 2 - A", &mut pad2.a);
                        });
                    });
                });
            });
        }
    }
    fn make_button_combo(&mut self, ui: &mut egui::Ui, name: &str, key_to_map: &mut VirtualKeyCode) {
        egui::ComboBox::from_label(name).selected_text(format!("{:?}", key_to_map))
        .show_ui(ui, |ui| {
            for available_code in AVAILABLE_KEY_CODES.iter() {
                ui.selectable_value(key_to_map, *available_code, format!("{:?}", available_code));
            }
        });
    }

    /// Render egui.
    pub(crate) fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        render_target: &wgpu::TextureView,
        context: &PixelsContext
    ) -> Result<(), BackendError> {
        // Upload all resources to the GPU.
        self.rpass.update_texture(
            &context.device,
            &context.queue,
            &self.platform.context().texture(),
        );
        self.rpass
            .update_user_textures(&context.device, &context.queue);
        self.rpass.update_buffers(
            &context.device,
            &context.queue,
            &self.paint_jobs,
            &self.screen_descriptor,
        );

        // Record all render passes.
        self.rpass.execute(
            encoder,
            render_target,
            &self.paint_jobs,
            &self.screen_descriptor,
            None,
        )
    }
}