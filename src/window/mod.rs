use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use anyhow::Result;
use thingbuf::{Recycle, ThingBuf};
use winit::event_loop::EventLoop;

use crate::{
    input::keys::{KeyCode, Modifiers},
    NES_HEIGHT, NES_WIDTH,
};

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

pub async fn create_renderer(
    title: &str,
    inner_size: Size,
    min_inner_size: Size,
    event_loop: &EventLoop<()>,
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

    Renderer::new(Arc::new(window_builder.build(event_loop)?)).await
}

#[derive(Debug, Clone)]
#[must_use]
pub struct NESFrame(Vec<u8>);

impl NESFrame {
    pub const SIZE: usize = (NES_WIDTH * NES_HEIGHT * 4) as usize;

    /// Allocate a new frame for video output.
    pub fn new() -> Self {
        let mut frame = vec![0; Self::SIZE];
        frame
            .iter_mut()
            .skip(3)
            .step_by(4)
            .for_each(|alpha| *alpha = 255);
        Self(frame)
    }
}

impl Default for NESFrame {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for NESFrame {
    type Target = Vec<u8>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NESFrame {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug)]
pub struct NESFramePool(Arc<ThingBuf<NESFrame, NESFrameRecycle>>);
#[derive(Debug)]
pub struct NESFrameRecycle;

impl Recycle<NESFrame> for NESFrameRecycle {
    fn new_element(&self) -> NESFrame {
        NESFrame::new()
    }

    fn recycle(&self, _frame: &mut NESFrame) {}
}

impl NESFramePool {
    pub fn new() -> Self {
        Self(Arc::new(ThingBuf::with_recycle(2, NESFrameRecycle)))
    }
}

impl Default for NESFramePool {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for NESFramePool {
    type Target = ThingBuf<NESFrame, NESFrameRecycle>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Clone for NESFramePool {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}
