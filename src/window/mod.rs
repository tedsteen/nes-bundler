use std::{mem::size_of, ops::Deref, sync::Arc};

use anyhow::Result;
use thingbuf::{Recycle, ThingBuf};
use winit::event_loop::EventLoop;

use crate::input::keys::{KeyCode, Modifiers};

use self::egui_winit_wgpu::Renderer;
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
    let window_builder = winit::window::WindowBuilder::new()
        .with_resizable(true)
        .with_inner_size(inner_size)
        .with_min_inner_size(min_inner_size)
        .with_title(title)
        .with_visible(true);

    #[cfg(windows)]
    let window_builder = {
        use winit::platform::windows::IconExtWindows;
        window_builder.with_window_icon(Some(winit::window::Icon::from_resource(1, None)?))
    };

    Renderer::new(Arc::new(window_builder.build(event_loop)?), frame_pool).await
}

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
