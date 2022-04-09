use egui::{Context, ClippedMesh, FontDefinitions, Style, TexturesDelta};
use egui_wgpu_backend::{BackendError, RenderPass, ScreenDescriptor};
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

pub(crate) struct Gui {
    // State for egui.
    ctx: Context,
    state: egui_winit::State,
    screen_descriptor: ScreenDescriptor,
    rpass: RenderPass,
    paint_jobs: Vec<ClippedMesh>,
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
        let ctx = Context::default();
        ctx.set_fonts(FontDefinitions::default());
        ctx.set_style(Style::default());

        let max_texture_size = pixels.device().limits().max_texture_dimension_2d as usize;
        let window_size = window.inner_size();
        let scale_factor = window.scale_factor() as f32;

        let textures = TexturesDelta::default();
        Self {
            state: egui_winit::State::from_pixels_per_point(max_texture_size, scale_factor),
            ctx,
            screen_descriptor: ScreenDescriptor {
                physical_width: window_size.width,
                physical_height: window_size.height,
                scale_factor: window.scale_factor() as f32,
            },
            rpass: RenderPass::new(pixels.device(), pixels.render_texture_format(), 1),
            paint_jobs: Vec::new(),

            show_gui: false,
            textures,
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
            self.screen_descriptor.physical_width = size.width;
            self.screen_descriptor.physical_height = size.height;
        }
        self.settings_gui.handle_event(event, settings);
        #[cfg(feature = "netplay")]
        self.netplay_gui.handle_event(event);
        self.state.on_event(&self.ctx, event);
    }

    /// Prepare egui.
    pub(crate) fn prepare(
        &mut self,
        window: &Window,
        settings: &mut Settings,
        #[cfg(feature = "netplay")]
        game_runner_state: &mut GameRunnerState,
    ) {
        let output = self.ctx.clone().run(self.state.take_egui_input(window), |ctx| {
            if self.show_gui {
                self.ui(&ctx, settings, #[cfg(feature = "netplay")] game_runner_state);
            }
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
    ) -> Result<(), BackendError> {
        // Upload all resources to the GPU.
        self.rpass
            .add_textures(&context.device, &context.queue, &self.textures)?;
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
        )?;

        // Cleanup
        let textures = std::mem::take(&mut self.textures);
        self.rpass.remove_textures(textures)
    }
}
