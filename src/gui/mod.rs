use egui::{ClippedMesh, FontDefinitions};
use egui_wgpu_backend::{BackendError, RenderPass, ScreenDescriptor};
use egui_winit_platform::{Platform, PlatformDescriptor};
use pixels::{wgpu, PixelsContext};
use std::time::Instant;
use winit::{window::Window};

use crate::network::p2p::P2P;
use crate::{GameRunnerState, Settings};

use self::netplay::NetplayGui;
use self::settings::SettingsGui;

mod settings;
mod netplay;

/// Manages all state required for rendering egui over `Pixels`.
pub(crate) struct Gui {
    // State for egui.
    start_time: Instant,
    platform: Platform,
    screen_descriptor: ScreenDescriptor,
    rpass: RenderPass,
    paint_jobs: Vec<ClippedMesh>,

    // State for the demo app.
    pub(crate) show_gui: bool,
    settings_gui: SettingsGui,
    netplay_gui: NetplayGui
}

impl Gui {
    /// Create egui.
    pub(crate) fn new(width: u32, height: u32, scale_factor: f64, pixels: &pixels::Pixels, p2p: P2P) -> Self {
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
            settings_gui: SettingsGui::new(),
            netplay_gui: NetplayGui::new(p2p)
        }
    }

    /// Handle input events from the window manager.
    pub(crate) fn handle_event(&mut self, event: &winit::event::Event<'_, ()>, settings: &mut Settings) {
        self.settings_gui.handle_event(event, settings);
        self.netplay_gui.handle_event(event);
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
    pub(crate) fn prepare(&mut self, window: &Window, settings: &mut Settings, game_runner_state: &mut GameRunnerState) {
        self.platform
            .update_time(self.start_time.elapsed().as_secs_f64());

        // Begin the egui frame.
        self.platform.begin_frame();

        if self.show_gui {
            self.ui(&self.platform.context(), settings, game_runner_state);
        }

        // End the egui frame and create all paint jobs to prepare for rendering.
        let (_output, paint_commands) = self.platform.end_frame(Some(window));
        self.paint_jobs = self.platform.context().tessellate(paint_commands);
    }
    
    // Draw all the things.
    fn ui(&mut self, ctx: &egui::CtxRef, settings: &mut Settings, game_runner_state: &mut GameRunnerState) {
        self.settings_gui.ui(ctx, settings);
        self.netplay_gui.ui(ctx, game_runner_state);
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