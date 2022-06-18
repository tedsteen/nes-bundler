use egui_winit::egui as egui;
use egui::{Context, ClippedPrimitive, TexturesDelta};
use egui_wgpu::renderer::{RenderPass, ScreenDescriptor};
use pixels::{wgpu, PixelsContext};
use egui_winit::winit as winit;
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

pub(crate) struct Gui {
    // State for egui.
    ctx: Context,
    state: egui_winit::State,
    screen_descriptor: ScreenDescriptor,
    rpass: RenderPass,
    paint_jobs: Vec<ClippedPrimitive>,
    textures: TexturesDelta,

    // State for the demo app.
    pub(crate) show_gui: bool,
    settings_gui: SettingsGui,
    #[cfg(feature = "netplay")]
    netplay_gui: NetplayGui,
}
// Render egui over pixels
impl Gui {
    pub(crate) fn new(window: &winit::window::Window, pixels: &pixels::Pixels, #[cfg(feature = "netplay")] p2p: P2P) -> Self {
        let max_texture_size = pixels.device().limits().max_texture_dimension_2d as usize;

        let window_size = window.inner_size();
        let width = window_size.width;
        let height = window_size.height;
        let scale_factor = window.scale_factor() as f32;

        Self {
            ctx: Context::default(),
            state: egui_winit::State::from_pixels_per_point(max_texture_size, scale_factor),
            screen_descriptor: ScreenDescriptor {
                size_in_pixels: [width, height],
                pixels_per_point: scale_factor,
            },
            rpass: RenderPass::new(pixels.device(), pixels.render_texture_format(), 1),
            paint_jobs: Vec::new(),

            show_gui: false,
            textures: TexturesDelta::default(),
            settings_gui: SettingsGui::new(),
            #[cfg(feature = "netplay")]
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
            if size.width > 0 && size.height > 0 {
                self.screen_descriptor.size_in_pixels = [size.width, size.height];
            }
        }
        //TODO: Check if scale factor changes
        self.settings_gui.handle_event(event, settings);
        #[cfg(feature = "netplay")]
        self.netplay_gui.handle_event(event);
        self.state.on_event(&self.ctx, event);
    }

    pub(crate) fn prepare(
        &mut self,
        window: &Window,
        settings: &mut Settings,
        #[cfg(feature = "netplay")]
        game_runner_state: &mut GameRunnerState) {
        // Run the egui frame and create all paint jobs to prepare for rendering.
        let raw_input = self.state.take_egui_input(window);
        let output = self.ctx.clone().run(raw_input, |egui_ctx| {
            // Draw the demo application.
            self.ui(egui_ctx, settings, #[cfg(feature = "netplay")] game_runner_state);
        });

        self.textures.append(output.textures_delta);
        self.state
            .handle_platform_output(window, &self.ctx, output.platform_output);
        self.paint_jobs = self.ctx.tessellate(output.shapes);
    }

    // Draw all ui
    fn ui(
        &mut self,
        ctx: &Context,
        settings: &mut Settings,
        #[cfg(feature = "netplay")]
        game_runner_state: &mut GameRunnerState,
    ) {
        self.settings_gui.ui(ctx, settings);
        #[cfg(feature = "netplay")]
        self.netplay_gui.ui(ctx, game_runner_state);
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
            self.rpass
                .update_texture(&context.device, &context.queue, *id, image_delta);
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
