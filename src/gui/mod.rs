use egui_winit::{winit::event::VirtualKeyCode};
use egui::{Context, ClippedPrimitive, TexturesDelta};
use egui_wgpu::renderer::{RenderPass, ScreenDescriptor};
use pixels::{wgpu, PixelsContext};
use winit::window::Window;

#[cfg(feature = "netplay")]
use crate::network::p2p::P2P;
#[cfg(feature = "netplay")]
use crate::GameRunnerState;
use crate::{Settings};

#[cfg(feature = "netplay")]
use self::netplay::NetplayGui;
use self::settings::SettingsGui;

#[cfg(feature = "netplay")]
mod netplay;
mod settings;
/// Manages all state required for rendering egui over `Pixels`.
pub(crate) struct Framework {
    // State for egui.
    egui_ctx: Context,
    egui_state: egui_winit::State,
    screen_descriptor: ScreenDescriptor,
    rpass: RenderPass,
    paint_jobs: Vec<ClippedPrimitive>,
    textures: TexturesDelta,

    // State for the GUI
    gui: Gui,
}
pub(crate) struct Gui {
    // State for the demo app.
    visible: bool,
    settings: SettingsGui,
    #[cfg(feature = "netplay")]
    netplay: NetplayGui    
}

// Render egui over pixels
impl Framework {
    /// Create egui.
    pub(crate) fn new(width: u32, height: u32, scale_factor: f32, pixels: &pixels::Pixels, #[cfg(feature = "netplay")] p2p: P2P) -> Self {
        let max_texture_size = pixels.device().limits().max_texture_dimension_2d as usize;

        let egui_ctx = Context::default();
        let egui_state = egui_winit::State::from_pixels_per_point(max_texture_size, scale_factor);
        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [width, height],
            pixels_per_point: scale_factor,
        };
        let rpass = RenderPass::new(pixels.device(), pixels.render_texture_format(), 1);
        let textures = TexturesDelta::default();
        let gui = Gui::new(p2p);

        Self {
            egui_ctx,
            egui_state,
            screen_descriptor,
            rpass,
            paint_jobs: Vec::new(),
            textures,
            gui,
        }
    }

    /// Handle input events from the window manager.
    pub(crate) fn handle_event(&mut self, event: &winit::event::WindowEvent, settings: &mut Settings) {
        self.egui_state.on_event(&self.egui_ctx, event);
        self.gui.handle_event(event, settings);
    }

    /// Resize egui.
    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.screen_descriptor.size_in_pixels = [width, height];
        }
    }

    /// Update scaling factor.
    pub(crate) fn scale_factor(&mut self, scale_factor: f64) {
        self.screen_descriptor.pixels_per_point = scale_factor as f32;
    }

    /// Prepare egui.
    pub(crate) fn prepare(&mut self,
        window: &Window,
        settings: &mut Settings,
        #[cfg(feature = "netplay")]
        game_runner_state: &mut GameRunnerState) {
        // Run the egui frame and create all paint jobs to prepare for rendering.
        let raw_input = self.egui_state.take_egui_input(window);
        let output = self.egui_ctx.run(raw_input, |egui_ctx| {
            self.gui.ui(egui_ctx, settings, #[cfg(feature = "netplay")] game_runner_state);
        });

        self.textures.append(output.textures_delta);
        self.egui_state.handle_platform_output(window, &self.egui_ctx, output.platform_output);
        self.paint_jobs = self.egui_ctx.tessellate(output.shapes);
    }

    /// Render egui.
    pub(crate) fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        render_target: &wgpu::TextureView,
        context: &PixelsContext,
    ) {
        // Upload all resources to the GPU.
        for (id, image_delta) in &self.textures.set {
            self.rpass.update_texture(&context.device, &context.queue, *id, image_delta);
        }
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
        );

        // Cleanup
        let textures = std::mem::take(&mut self.textures);
        for id in &textures.free {
            self.rpass.free_texture(id);
        }
    }
}

impl Gui {
    fn new(#[cfg(feature = "netplay")] p2p: P2P) -> Self {
        Self {
            visible: false,
            settings: SettingsGui::new(),
            #[cfg(feature = "netplay")]
            netplay: NetplayGui::new(p2p),
        }
    }

    fn handle_event(&mut self, event: &winit::event::WindowEvent, settings: &mut Settings) {
        if let winit::event::WindowEvent::KeyboardInput { input, .. } = event {
            if let Some(code) = input.virtual_keycode {
                if input.state == winit::event::ElementState::Pressed && code == VirtualKeyCode::Escape {
                    self.visible = !self.visible;
                }
            }
        }
        self.settings.handle_event(event, settings);
        #[cfg(feature = "netplay")]
        self.netplay.handle_event(event);
    }

    fn ui(&mut self,
        ctx: &Context,
        settings: &mut Settings,
        #[cfg(feature = "netplay")]
        game_runner_state: &mut GameRunnerState) {
            if self.visible {
                self.settings.ui(ctx, settings);
                #[cfg(feature = "netplay")]
                self.netplay.ui(ctx, game_runner_state);
            }
        }
}