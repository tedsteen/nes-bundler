use std::{num::NonZeroU32, sync::Arc};

use anyhow::Result;
use egui::NumExt;
use glutin::config::ConfigSurfaceTypes;
use image::DynamicImage;
use raw_window_handle::HasRawWindowHandle;
use winit::window::Icon;

use crate::input::keys::{KeyCode, Modifiers};
mod winit_impl;

pub trait Fullscreen {
    fn check_and_set_fullscreen(&mut self, key_mod: &Modifiers, key_code: &KeyCode) -> bool;
}

pub struct GlutinWindowContext {
    window: winit::window::Window,
    gl_context: glutin::context::PossiblyCurrentContext,
    gl_surface: glutin::surface::Surface<glutin::surface::WindowSurface>,
    pub glow_context: Arc<glow::Context>,
}
unsafe impl Send for GlutinWindowContext {}

pub struct Size {
    pub width: f64,
    pub height: f64,
}
impl Size {
    pub(crate) fn new(width: f64, height: f64) -> Size {
        Self { width, height }
    }
}

impl From<Size> for winit::dpi::Size {
    fn from(val: Size) -> Self {
        winit::dpi::Size::Logical(winit::dpi::LogicalSize {
            width: val.width,
            height: val.height,
        })
    }
}

impl GlutinWindowContext {
    // refactor this function to use `glutin-winit` crate eventually.
    // preferably add android support at the same time.
    pub fn new(
        title: &str,
        window_icon: Option<DynamicImage>,
        inner_size: Size,
        min_inner_size: Size,
        event_loop: &winit::event_loop::EventLoopWindowTarget<()>,
    ) -> Result<Self> {
        use glutin::display::GetGlDisplay;
        use glutin::display::GlDisplay;
        use glutin::prelude::GlSurface;

        let winit_window_builder = winit::window::WindowBuilder::new()
            .with_resizable(true)
            .with_window_icon(window_icon.map(|image| {
                let image = image.into_rgba8();
                let (width, height) = image.dimensions();
                Icon::from_rgba(image.into_raw(), width, height).expect("Failed to open icon")
            }))
            //.with_disallow_hidpi(true)
            .with_inner_size(inner_size)
            .with_min_inner_size(min_inner_size)
            .with_title(title)
            .with_visible(true);

        let config_template_builder = glutin::config::ConfigTemplateBuilder::new()
            .prefer_hardware_accelerated(Some(true))
            .with_depth_size(0)
            .with_stencil_size(0)
            .with_surface_type(ConfigSurfaceTypes::WINDOW)
            //.with_swap_interval(1, 1)
            .with_transparency(false);

        log::debug!("trying to get gl_config");
        let (mut window, gl_config) =
            glutin_winit::DisplayBuilder::new() // let glutin-winit helper crate handle the complex parts of opengl context creation
                .with_preference(glutin_winit::ApiPreference::FallbackEgl) // https://github.com/emilk/egui/issues/2520#issuecomment-1367841150
                .with_window_builder(Some(winit_window_builder.clone()))
                .build(
                    event_loop,
                    config_template_builder,
                    |mut config_iterator| {
                        config_iterator.next().expect(
                            "failed to find a matching configuration for creating glutin config",
                        )
                    },
                )
                .expect("failed to create gl_config");
        let gl_display = gl_config.display();
        log::debug!("found gl_config: {:?}", &gl_config);

        let raw_window_handle = window.as_ref().map(|w| w.raw_window_handle());
        log::debug!("raw window handle: {:?}", raw_window_handle);
        let context_attributes =
            glutin::context::ContextAttributesBuilder::new().build(raw_window_handle);
        // by default, glutin will try to create a core opengl context. but, if it is not available, try to create a gl-es context using this fallback attributes
        let fallback_context_attributes = glutin::context::ContextAttributesBuilder::new()
            .with_context_api(glutin::context::ContextApi::Gles(None))
            .build(raw_window_handle);
        let not_current_gl_context = unsafe {
            gl_display
                    .create_context(&gl_config, &context_attributes)
                    .unwrap_or_else(|_| {
                        log::debug!("failed to create gl_context with attributes: {:?}. retrying with fallback context attributes: {:?}",
                            &context_attributes,
                            &fallback_context_attributes);
                        gl_config
                            .display()
                            .create_context(&gl_config, &fallback_context_attributes)
                            .expect("failed to create context even with fallback attributes")
                    })
        };

        // this is where the window is created, if it has not been created while searching for suitable gl_config
        let window = window.take().unwrap_or_else(|| {
            log::debug!("window doesn't exist yet. creating one now with finalize_window");
            glutin_winit::finalize_window(event_loop, winit_window_builder.clone(), &gl_config)
                .expect("failed to finalize glutin window")
        });
        let (width, height): (u32, u32) = window.inner_size().into();
        let width = NonZeroU32::new(width.at_least(1)).unwrap();
        let height = NonZeroU32::new(height.at_least(1)).unwrap();
        let surface_attributes =
            glutin::surface::SurfaceAttributesBuilder::<glutin::surface::WindowSurface>::new()
                .build(window.raw_window_handle(), width, height);
        log::debug!(
            "creating surface with attributes: {:?}",
            &surface_attributes
        );
        let gl_surface =
            unsafe { gl_display.create_window_surface(&gl_config, &surface_attributes)? };
        log::debug!("surface created successfully: {gl_surface:?}.making context current");
        let gl_context = glutin::context::NotCurrentGlContext::make_current(
            not_current_gl_context,
            &gl_surface,
        )?;

        gl_surface.set_swap_interval(
            &gl_context,
            glutin::surface::SwapInterval::Wait(NonZeroU32::new(1).unwrap()),
        )?;
        //gl_surface.set_swap_interval(&gl_context, glutin::surface::SwapInterval::DontWait)?;

        #[allow(clippy::arc_with_non_send_sync)]
        Ok(GlutinWindowContext {
            window,
            gl_context,
            glow_context: Arc::new(unsafe {
                glow::Context::from_loader_function(|s| {
                    let s = std::ffi::CString::new(s)
                        .expect("failed to construct C string from string for gl proc address");
                    gl_display.get_proc_address(&s)
                })
            }),
            gl_surface,
        })
    }

    pub fn window(&self) -> &winit::window::Window {
        &self.window
    }

    pub fn window_mut(&mut self) -> &mut winit::window::Window {
        &mut self.window
    }

    pub fn resize(&self, physical_size: winit::dpi::PhysicalSize<u32>) {
        use glutin::surface::GlSurface;
        self.gl_surface.resize(
            &self.gl_context,
            NonZeroU32::new(physical_size.width.at_least(1)).unwrap(),
            NonZeroU32::new(physical_size.height.at_least(1)).unwrap(),
        );
    }

    pub fn swap_buffers(&self) -> glutin::error::Result<()> {
        use glutin::surface::GlSurface;
        self.gl_surface.swap_buffers(&self.gl_context)
    }

    pub fn get_dpi(&self) -> f32 {
        self.window.scale_factor() as f32
    }
}

pub fn create_display(
    title: &str,
    window_icon: Option<DynamicImage>,
    inner_size: Size,
    min_inner_size: Size,
    event_loop: &winit::event_loop::EventLoopWindowTarget<()>,
) -> Result<GlutinWindowContext> {
    GlutinWindowContext::new(title, window_icon, inner_size, min_inner_size, event_loop)
}
