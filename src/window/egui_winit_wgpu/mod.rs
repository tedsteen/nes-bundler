mod gui;

use std::sync::Arc;

use anyhow::Result;
use egui::Ui;
use egui_wgpu::ScreenDescriptor;
use gui::EguiRenderer;
use wgpu::{CurrentSurfaceTexture, PresentMode};
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
        // All backends (the default); the display handle lets GL present on Wayland.
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(Arc::clone(&window)),
        ));
        let surface = instance.create_surface(Arc::clone(&window))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .expect("adapter to be crated");

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await?;

        let surface_caps = surface.get_capabilities(&adapter);

        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            // egui prefers Rgba8Unorm or Bgra8Unorm
            .find(|f| !f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        // Always vsync: presenting waits for the display, and the display sets the pace of the
        // emulation (see `AppShell`).
        let present_mode = PresentMode::AutoVsync;

        let config = wgpu::SurfaceConfiguration {
            desired_maximum_frame_latency: 1,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode,
            alpha_mode: surface_caps.alpha_modes[0],
            color_space: wgpu::SurfaceColorSpace::Auto,
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

    /// Renders and presents a frame, or skips it when the surface has no texture to give (e.g. the
    /// window is occluded, or the surface had to be reconfigured). Returns whether it presented.
    pub fn render(&mut self, mut run_ui: impl FnMut(&mut Ui)) -> bool {
        #[cfg(feature = "debug")]
        puffin::profile_function!();

        let (output, suboptimal) = {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("get_current_texture");

            match self.surface.get_current_texture() {
                CurrentSurfaceTexture::Success(texture) => (texture, false),
                CurrentSurfaceTexture::Suboptimal(texture) => (texture, true),
                CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => return false,
                CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                    log::warn!("Surface lost or outdated, reconfiguring.");
                    self.surface.configure(&self.device, &self.config);
                    return false;
                }
                CurrentSurfaceTexture::Validation => {
                    log::warn!("Surface validation error, skipping frame.");
                    return false;
                }
            }
        };

        let view = {
            #[cfg(feature = "debug")]
            puffin::profile_scope!("create_view");

            output.texture.create_view(&Default::default())
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

            self.queue.present(output);
        }

        if suboptimal {
            self.surface.configure(&self.device, &self.config);
        }
        true
    }
}
