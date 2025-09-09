mod gui;

use std::sync::Arc;

use anyhow::Result;
use egui::Context;
use egui_wgpu::ScreenDescriptor;
use gui::EguiRenderer;
use wgpu::{PresentMode, TextureViewDescriptor};
use winit::window::Window;

use crate::bundle::Bundle;

pub mod texture;

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    pub queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,

    pub window: Arc<Window>,
    pub egui: gui::EguiRenderer,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let surface = instance.create_surface(Arc::clone(&window))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("adapter to be crated");

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                trace: wgpu::Trace::Off,
                label: None,
                required_features: wgpu::Features::empty(),
                // WebGL doesn't support all of wgpu's features, so if
                // we're building for the web we'll have to disable some.
                required_limits: Default::default(),
                memory_hints: Default::default(),
            })
            .await?;

        let surface_caps = surface.get_capabilities(&adapter);

        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            // egui prefers Rgba8Unorm or Bgra8Unorm
            .find(|f| !f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let present_mode = if Bundle::current().config.enable_vsync {
            PresentMode::AutoVsync
        } else {
            [
                PresentMode::Mailbox,
                PresentMode::Immediate,
                PresentMode::Fifo,
            ]
            .into_iter()
            .find(|mode| surface_caps.present_modes.contains(mode))
            .unwrap_or(PresentMode::AutoNoVsync)
        };

        let config = wgpu::SurfaceConfiguration {
            desired_maximum_frame_latency: 1,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
        };
        log::debug!("Surface configuration: {config:?}");
        surface.configure(&device, &config);

        let egui = EguiRenderer::new(&device, config.format, None, 1, &window);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            size,
            window,
            egui,
        })
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn render(&mut self, mut run_ui: impl FnMut(&Context)) -> Result<(), wgpu::SurfaceError> {
        #[cfg(feature = "debug")]
        puffin::profile_function!();

        let output = {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("get_current_texture");

            self.surface.get_current_texture()?
        };

        let view = {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("create_view");

            output.texture.create_view(&TextureViewDescriptor {
                label: None,
                format: None,
                dimension: None,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
                usage: None,
            })
        };

        let mut encoder = {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("create_command_encoder");

            self.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Render Encoder"),
                })
        };

        let screen_descriptor = {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("ScreenDescriptor");

            ScreenDescriptor {
                size_in_pixels: [self.config.width, self.config.height],
                pixels_per_point: self.window().scale_factor() as f32,
            }
        };

        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("egui.draw");

            self.egui.draw(
                &self.device,
                &self.queue,
                &mut encoder,
                &self.window,
                &view,
                screen_descriptor,
                |ui| {
                    #[cfg(feature = "debug")]
                    {
                        puffin_egui::show_viewport_if_enabled(ui);
                        puffin::GlobalProfiler::lock().new_frame();
                    }

                    run_ui(ui)
                },
            );
        }
        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("submit");

            self.queue.submit(std::iter::once(encoder.finish()));
        }
        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("present");

            output.present();
        }

        Ok(())
    }
}
