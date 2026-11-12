use egui::epaint::Shadow;
use egui::{Context, Theme, Ui, Visuals};
use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor};

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
        // egui 0.34 made buttons and widgets roomier; keep the sizes the menus were laid out for.
        egui_context.all_styles_mut(|style| {
            style.spacing.button_padding = egui::vec2(4.0, 1.0);
            style.spacing.interact_size.y = 18.0;
        });

        let egui_state =
            egui_winit::State::new(egui_context.clone(), id, &window, None, None, None);

        // egui_state.set_pixels_per_point(window.scale_factor() as f32);
        let egui_renderer = Renderer::new(
            device,
            output_color_format,
            RendererOptions {
                depth_stencil_format: output_depth_format,
                msaa_samples,
                dithering: false,
                ..Default::default()
            },
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
        run_ui: impl FnMut(&mut Ui),
    ) {
        let raw_input = self.state.take_egui_input(window);
        let mut full_output = self.context.run_ui(raw_input, run_ui);

        self.state
            .handle_platform_output(window, full_output.platform_output);

        let tris = self
            .context
            .tessellate(full_output.shapes, full_output.pixels_per_point);
        // egui panics if a texture delta is dropped without being applied, so drain them.
        for (id, image_deltas) in full_output.textures_delta.set.drain() {
            for image_delta in image_deltas {
                self.renderer
                    .update_texture(device, queue, id, &image_delta);
            }
        }
        self.renderer
            .update_buffers(device, queue, encoder, &tris, &screen_descriptor);
        let rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: window_surface_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            label: Some("egui main render pass"),
            ..Default::default()
        });
        // Forgetting the pass' lifetime means that we are no longer compile-time protected from
        // runtime errors caused by accessing the parent encoder before the render pass is dropped.
        // Since we don't pass it on to the renderer, we should be perfectly safe against this mistake here!
        self.renderer
            .render(&mut rpass.forget_lifetime(), &tris, &screen_descriptor);

        for id in full_output.textures_delta.free.drain() {
            self.renderer.free_texture(&id)
        }
    }
}
