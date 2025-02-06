use egui::epaint::Shadow;
use egui::{Context, Theme, Visuals};
use egui_wgpu::{Renderer, ScreenDescriptor};

use egui_winit::{EventResponse, State};
use wgpu::{CommandEncoder, Device, Queue, TextureFormat, TextureView};
use winit::event::WindowEvent;
use winit::window::Window;

pub struct EguiRenderer {
    context: Context,
    pub state: State,
    pub renderer: Renderer,
}

impl EguiRenderer {
    pub fn new(
        device: &Device,
        output_color_format: TextureFormat,
        output_depth_format: Option<TextureFormat>,
        msaa_samples: u32,
        window: &Window,
    ) -> EguiRenderer {
        let egui_context = Context::default();
        let id = egui_context.viewport_id();

        let visuals = Visuals {
            window_shadow: Shadow::NONE,
            slider_trailing_fill: true,
            handle_shape: egui::style::HandleShape::Rect { aspect_ratio: 0.6 },
            ..Default::default()
        };

        egui_context.set_visuals_of(Theme::Dark, visuals);

        let egui_state =
            egui_winit::State::new(egui_context.clone(), id, &window, None, None, None);

        // egui_state.set_pixels_per_point(window.scale_factor() as f32);
        let egui_renderer = Renderer::new(
            device,
            output_color_format,
            output_depth_format,
            msaa_samples,
            false,
        );

        EguiRenderer {
            context: egui_context,
            state: egui_state,
            renderer: egui_renderer,
        }
    }

    pub fn handle_input(&mut self, window: &Window, event: &WindowEvent) -> EventResponse {
        self.state.on_window_event(window, event)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        window: &Window,
        window_surface_view: &TextureView,
        screen_descriptor: ScreenDescriptor,
        run_ui: impl FnMut(&Context),
    ) {
        let raw_input = self.state.take_egui_input(window);
        let full_output = self.context.run(raw_input, run_ui);

        self.state
            .handle_platform_output(window, full_output.platform_output);

        let tris = self
            .context
            .tessellate(full_output.shapes, full_output.pixels_per_point);
        for (id, image_delta) in &full_output.textures_delta.set {
            self.renderer
                .update_texture(device, queue, *id, image_delta);
        }
        self.renderer
            .update_buffers(device, queue, encoder, &tris, &screen_descriptor);
        let rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: window_surface_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            label: Some("egui main render pass"),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        // Forgetting the pass' lifetime means that we are no longer compile-time protected from
        // runtime errors caused by accessing the parent encoder before the render pass is dropped.
        // Since we don't pass it on to the renderer, we should be perfectly safe against this mistake here!
        self.renderer
            .render(&mut rpass.forget_lifetime(), &tris, &screen_descriptor);

        for x in &full_output.textures_delta.free {
            self.renderer.free_texture(x)
        }
    }
}
