use egui_wgpu_backend::egui::{ClippedMesh, CtxRef, FontDefinitions, Style};
use egui_wgpu_backend::{BackendError, RenderPass, ScreenDescriptor};
use pixels::{wgpu, PixelsContext};
use winit::window::Window;

use crate::network::p2p::P2P;
use crate::{GameRunnerState, Settings};

use self::netplay::NetplayGui;
use self::settings::SettingsGui;

mod netplay;
mod settings;

pub(crate) struct Gui {
    // State for egui.
    state: egui_winit::State,
    ctx: CtxRef,
    screen_descriptor: ScreenDescriptor,
    rpass: RenderPass,
    paint_jobs: Vec<ClippedMesh>,

    // State for the demo app.
    pub(crate) show_gui: bool,
    settings_gui: SettingsGui,
    netplay_gui: NetplayGui,
}
// Render egui over pixels
impl Gui {
    pub(crate) fn new(window: &winit::window::Window, pixels: &pixels::Pixels, p2p: P2P) -> Self {
        let ctx = CtxRef::default();
        ctx.set_fonts(FontDefinitions::default());
        ctx.set_style(Style::default());

        let window_size = window.inner_size();
        Self {
            state: egui_winit::State::new(window),
            ctx,
            screen_descriptor: ScreenDescriptor {
                physical_width: window_size.width,
                physical_height: window_size.height,
                scale_factor: window.scale_factor() as f32,
            },
            rpass: RenderPass::new(pixels.device(), pixels.render_texture_format(), 1),
            paint_jobs: Vec::new(),

            show_gui: false,
            settings_gui: SettingsGui::new(),
            netplay_gui: NetplayGui::new(p2p),
        }
    }

    /// Handle input events from winit
    pub(crate) fn handle_event(
        &mut self,
        event: &winit::event::WindowEvent,
        settings: &mut Settings,
    ) {
        if let winit::event::WindowEvent::Resized(size) = event {
            self.screen_descriptor.physical_width = size.width;
            self.screen_descriptor.physical_height = size.height;
        }
        self.settings_gui.handle_event(event, settings);
        self.netplay_gui.handle_event(event);
        self.state.on_event(&self.ctx, event);
    }

    /// Prepare egui.
    pub(crate) fn prepare(
        &mut self,
        window: &Window,
        settings: &mut Settings,
        game_runner_state: &mut GameRunnerState,
    ) {
        // Begin the egui frame.
        self.ctx.begin_frame(self.state.take_egui_input(window));

        if self.show_gui {
            self.ui(&self.ctx.clone(), settings, game_runner_state);
        }

        // End the egui frame and create all paint jobs to prepare for rendering.
        let (output, shapes) = self.ctx.end_frame();
        self.state.handle_output(window, &self.ctx, output);

        self.paint_jobs = self.ctx.tessellate(shapes);
    }

    // Draw all ui
    fn ui(
        &mut self,
        ctx: &CtxRef,
        settings: &mut Settings,
        game_runner_state: &mut GameRunnerState,
    ) {
        self.settings_gui.ui(ctx, settings);
        self.netplay_gui.ui(ctx, settings.audio_latency, game_runner_state);
    }

    /// Render egui.
    pub(crate) fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        render_target: &wgpu::TextureView,
        context: &PixelsContext,
    ) -> Result<(), BackendError> {
        // Upload all resources to the GPU.
        self.rpass
            .update_texture(&context.device, &context.queue, &self.ctx.texture());
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
