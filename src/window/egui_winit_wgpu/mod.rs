mod gui;

use std::{iter, sync::Arc};

use anyhow::Result;
use egui::Context;
use egui_wgpu::ScreenDescriptor;
use gui::EguiRenderer;
use wgpu::{PresentMode, TextureViewDescriptor};
use winit::window::Window;

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

        // The instance is a handle to our GPU
        // BackendBit::PRIMARY => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
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
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    // WebGL doesn't support all of wgpu's features, so if
                    // we're building for the web we'll have to disable some.
                    required_limits: wgpu::Limits::default(),
                },
                None, // Trace path
            )
            .await?;

        let surface_caps = surface.get_capabilities(&adapter);

        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            // egui prefers Rgba8Unorm or Bgra8Unorm
            .find(|f| !f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);
        log::debug!("Surface format: {surface_format:?}");

        let config = wgpu::SurfaceConfiguration {
            desired_maximum_frame_latency: 1,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            //present_mode: PresentMode::AutoVsync,
            present_mode: PresentMode::AutoNoVsync,
            //present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        // ...
        let egui = EguiRenderer::new(
            &device,       // wgpu Device
            config.format, // TextureFormat
            None,          // this can be None
            1,             // samples
            &window,       // winit Window
        );

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

    pub fn render(&mut self, run_ui: impl FnOnce(&Context)) -> Result<(), wgpu::SurfaceError> {
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

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point: self.window().scale_factor() as f32,
        };
        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("draw");
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
                        puffin::GlobalProfiler::lock().new_frame();
                        puffin_egui::show_viewport_if_enabled(ui);
                    }

                    run_ui(ui)
                },
            );
        }
        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("queue.submit");
            self.queue.submit(iter::once(encoder.finish()));
        }
        {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("output.present");
            output.present();
        }

        Ok(())
    }
}
