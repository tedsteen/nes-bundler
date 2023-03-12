use self::{audio::AudioSettingsGui, input::InputSettingsGui};
use crate::GameRunner;
use egui::{ClippedPrimitive, Context, TexturesDelta};
use egui_wgpu::renderer::{Renderer, ScreenDescriptor};
use pixels::{wgpu, PixelsContext};
use winit::{event::VirtualKeyCode, event_loop::EventLoopWindowTarget, window::Window};

mod audio;
#[cfg(feature = "debug")]
mod debug;
mod input;
#[cfg(feature = "netplay")]
mod netplay;

/// Manages all state required for rendering egui over `Pixels`.
pub struct Framework {
    // State for egui.
    egui_ctx: Context,
    egui_state: egui_winit::State,
    screen_descriptor: ScreenDescriptor,
    renderer: Renderer,
    paint_jobs: Vec<ClippedPrimitive>,
    textures: TexturesDelta,

    // State for the GUI
    gui: Gui,
}

// Render egui over pixels
impl Framework {
    /// Create egui.
    pub fn new<T>(
        event_loop: &EventLoopWindowTarget<T>,
        width: u32,
        height: u32,
        scale_factor: f32,
        pixels: &pixels::Pixels,
    ) -> Self {
        let max_texture_size = pixels.device().limits().max_texture_dimension_2d as usize;

        let egui_ctx = Context::default();
        let mut egui_state = egui_winit::State::new(event_loop);
        egui_state.set_max_texture_side(max_texture_size);
        egui_state.set_pixels_per_point(scale_factor);
        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [width, height],
            pixels_per_point: scale_factor,
        };
        let renderer = Renderer::new(pixels.device(), pixels.render_texture_format(), None, 1);
        let textures = TexturesDelta::default();
        let gui = Gui::new();

        Self {
            egui_ctx,
            egui_state,
            screen_descriptor,
            renderer,
            paint_jobs: Vec::new(),
            textures,
            gui,
        }
    }

    /// Handle input events from the window manager.
    pub fn handle_event(
        &mut self,
        event: &winit::event::WindowEvent,
        game_runner: &mut GameRunner,
    ) {
        match event {
            winit::event::WindowEvent::ScaleFactorChanged {
                scale_factor,
                new_inner_size: _,
            } => {
                self.screen_descriptor.pixels_per_point = *scale_factor as f32;
            }
            winit::event::WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    self.screen_descriptor.size_in_pixels = [size.width, size.height];
                }
            }
            _ => {}
        }

        let _ = self.egui_state.on_event(&self.egui_ctx, event);
        self.gui.handle_event(event, game_runner);
    }

    /// Prepare egui.
    pub fn prepare(&mut self, window: &Window, game_runner: &mut GameRunner) {
        // Run the egui frame and create all paint jobs to prepare for rendering.
        let raw_input = self.egui_state.take_egui_input(window);
        let output = self.egui_ctx.run(raw_input, |egui_ctx| {
            self.gui.ui(egui_ctx, game_runner);
        });

        self.textures.append(output.textures_delta);
        self.egui_state
            .handle_platform_output(window, &self.egui_ctx, output.platform_output);
        self.paint_jobs = self.egui_ctx.tessellate(output.shapes);
    }

    /// Render egui.
    pub fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        render_target: &wgpu::TextureView,
        context: &PixelsContext,
    ) {
        // Upload all resources to the GPU.
        for (id, image_delta) in &self.textures.set {
            self.renderer
                .update_texture(&context.device, &context.queue, *id, image_delta);
        }
        self.renderer.update_buffers(
            &context.device,
            &context.queue,
            encoder,
            &self.paint_jobs,
            &self.screen_descriptor,
        );

        // Render egui with WGPU
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: render_target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
            });

            self.renderer
                .render(&mut rpass, &self.paint_jobs, &self.screen_descriptor);
        }

        // Cleanup
        let textures = std::mem::take(&mut self.textures);
        for id in &textures.free {
            self.renderer.free_texture(id);
        }
    }
}

trait GuiComponent {
    fn handle_event(&mut self, event: &winit::event::WindowEvent, game_runner: &mut GameRunner);
    fn ui(&mut self, ctx: &Context, game_runner: &mut GameRunner, ui_visible: bool);
    fn is_open(&mut self) -> &mut bool;
    fn name(&self) -> String;
}

pub struct Gui {
    // State for the demo app.
    visible: bool,
    settings: Vec<Box<dyn GuiComponent>>,
}

impl Gui {
    fn new() -> Self {
        let settings: Vec<Box<dyn GuiComponent>> = vec![
            Box::new(AudioSettingsGui::new()),
            Box::new(InputSettingsGui::new()),
            #[cfg(feature = "netplay")]
            Box::new(netplay::NetplayGui::new()),
            #[cfg(feature = "debug")]
            Box::new(debug::DebugGui::new()),
        ];
        Self {
            visible: false,
            settings,
        }
    }

    fn handle_event(&mut self, event: &winit::event::WindowEvent, game_runner: &mut GameRunner) {
        if let winit::event::WindowEvent::KeyboardInput { input, .. } = event {
            if let Some(code) = input.virtual_keycode {
                if input.state == winit::event::ElementState::Pressed
                    && code == VirtualKeyCode::Escape
                {
                    self.visible = !self.visible;
                }
            }
        }
        for g in &mut self.settings {
            g.handle_event(event, game_runner);
        }
    }

    fn ui(&mut self, ctx: &Context, game_runner: &mut GameRunner) {
        if self.visible {
            egui::TopBottomPanel::top("menubar_container").show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("Settings", |ui| {
                        for setting in &mut self.settings {
                            if ui.button(setting.name()).clicked() {
                                *setting.is_open() = !*setting.is_open();
                                ui.close_menu();
                            }
                        }
                    })
                });
            });
        }

        for setting in &mut self.settings {
            setting.ui(ctx, game_runner, self.visible);
        }
    }
}
