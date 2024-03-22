mod gui;

use std::{iter, mem::size_of, ops::Deref, sync::Arc};

use anyhow::Result;
use egui::Context;
use egui_wgpu::ScreenDescriptor;
use gui::EguiRenderer;
use thingbuf::{Recycle, ThingBuf};
use wgpu::{PresentMode, TextureViewDescriptor};
use winit::window::Window;

use crate::nes_state::VideoFrame;

#[derive(Debug)]
pub struct TFrameRecycle<const N: usize>;
impl<const N: usize> Recycle<[u8; N]> for TFrameRecycle<N> {
    fn new_element(&self) -> [u8; N] {
        [0; N]
    }

    fn recycle(&self, _frame: &mut [u8; N]) {}
}
#[derive(Debug)]
pub struct BytePool<const N: usize>(Arc<ThingBuf<[u8; N], TFrameRecycle<N>>>);

impl<const N: usize> BytePool<N> {
    pub fn new() -> Self {
        Self(Arc::new(ThingBuf::with_recycle(2, TFrameRecycle)))
    }
}

impl<const N: usize> Deref for BytePool<N> {
    type Target = Arc<ThingBuf<[u8; N], TFrameRecycle<N>>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const N: usize> Clone for BytePool<N> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<const N: usize> Default for BytePool<N> {
    fn default() -> Self {
        Self::new()
    }
}

const VIDEO_FRAME_SIZE: usize = size_of::<VideoFrame>();
pub type VideoFramePool = BytePool<VIDEO_FRAME_SIZE>;

pub struct Renderer {
    //TODO: Where should this live?
    pub frame_pool: VideoFramePool,

    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    // NEW!
    pub window: Arc<Window>,
    pub egui: gui::EguiRenderer,
}

impl Renderer {
    pub async fn new(window: Arc<Window>, frame_pool: VideoFramePool) -> Result<Self> {
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
            .unwrap();

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
            frame_pool,
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
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&TextureViewDescriptor {
            label: None,
            format: None,
            dimension: None,
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point: self.window().scale_factor() as f32,
        };

        self.egui.draw(
            &self.device,
            &self.queue,
            &mut encoder,
            &self.window,
            &view,
            screen_descriptor,
            |ui| run_ui(ui),
        );

        self.queue.submit(iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
