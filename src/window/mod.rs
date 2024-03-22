use std::sync::Arc;

use anyhow::Result;
use winit::event_loop::EventLoop;

use crate::input::keys::{KeyCode, Modifiers};

use self::egui_winit_wgpu::{Renderer, VideoFramePool};
pub mod egui_winit_wgpu;
mod winit_impl;

pub trait Fullscreen {
    fn check_and_set_fullscreen(&self, key_mod: &Modifiers, key_code: &KeyCode) -> bool;
}

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

pub async fn create_state(
    title: &str,
    inner_size: Size,
    min_inner_size: Size,
    event_loop: &EventLoop<()>,
    frame_pool: VideoFramePool,
) -> Result<Renderer> {
    let window = winit::window::WindowBuilder::new()
        .with_resizable(true)
        .with_inner_size(inner_size)
        .with_min_inner_size(min_inner_size)
        .with_title(title)
        .with_visible(true)
        .build(event_loop)?;

    #[cfg(windows)]
    let winit_window_builder = {
        use winit::platform::windows::IconExtWindows;
        winit_window_builder.with_window_icon(Some(winit::window::Icon::from_resource(1, None)?))
    };

    Renderer::new(Arc::new(window), frame_pool).await
}
