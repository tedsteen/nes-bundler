use crate::{
    input::keys::{KeyCode, Modifiers},
    Size,
};
use anyhow::Result;
use winit::{event_loop::ActiveEventLoop, window::Window};

pub mod egui_winit_wgpu;
mod winit_impl;

pub trait Fullscreen {
    fn check_and_set_fullscreen(&self, key_mod: Modifiers, key_code: KeyCode) -> bool;
    fn toggle_fullscreen(&self);
    fn is_fullscreen(&self) -> bool;
}

impl From<Size> for winit::dpi::Size {
    fn from(val: Size) -> Self {
        winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(
            val.width as f64,
            val.height as f64,
        ))
    }
}

pub fn create_window(
    title: &str,
    inner_size: Size,
    min_inner_size: Size,
    event_loop: &ActiveEventLoop,
) -> Result<winit::window::Window> {
    let window_attributes = Window::default_attributes()
        .with_resizable(true)
        .with_inner_size(inner_size)
        .with_min_inner_size(min_inner_size)
        .with_title(title)
        .with_visible(true);

    #[cfg(windows)]
    let window_attributes = {
        use winit::platform::windows::IconExtWindows;
        window_attributes.with_window_icon(Some(winit::window::Icon::from_resource(1, None)?))
    };
    Ok(event_loop.create_window(window_attributes).unwrap())
}
