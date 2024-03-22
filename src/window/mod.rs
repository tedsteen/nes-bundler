use std::sync::Arc;

use anyhow::Result;
use winit::event_loop::EventLoop;

use crate::input::keys::{KeyCode, Modifiers};

use self::egui_winit_wgpu::{State, VideoFramePool};
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
) -> Result<State> {
    let window = winit::window::WindowBuilder::new()
        .with_resizable(true)
        .with_inner_size(inner_size)
        .with_min_inner_size(min_inner_size)
        .with_title(title)
        .with_visible(true)
        .build(event_loop)?;

    #[cfg(windows)]
    {
        let raw_window_handle = wgpu::rwh::HasWindowHandle::window_handle(&window)
            .unwrap()
            .as_raw();

        //let raw_window_handle = window.has_window_handle().map(|w| w.raw_window_handle());
        log::debug!("raw window handle: {:?}", raw_window_handle);
        fn get_instance_handle() -> windows_sys::Win32::Foundation::HMODULE {
            // Gets the instance handle by taking the address of the
            // pseudo-variable created by the microsoft linker:
            // https://devblogs.microsoft.com/oldnewthing/20041025-00/?p=37483

            // This is preferred over GetModuleHandle(NULL) because it also works in DLLs:
            // https://stackoverflow.com/questions/21718027/getmodulehandlenull-vs-hinstance

            extern "C" {
                static __ImageBase: windows_sys::Win32::System::SystemServices::IMAGE_DOS_HEADER;
            }

            unsafe { &__ImageBase as *const _ as _ }
        }

        if let raw_window_handle::RawWindowHandle::Win32(w) = raw_window_handle {
            use windows_sys::Win32::UI::WindowsAndMessaging::*;
            let instance_handle = get_instance_handle();
            log::debug!("Got instance handle: {:?}", instance_handle);
            let icon_handle = unsafe {
                LoadIconW(
                    instance_handle,
                    1 as usize as windows_sys::core::PCWSTR, /* MAKEINTRESOURCEW */
                )
            };
            log::debug!("Got icon handle: {:?}", icon_handle);
            if icon_handle != 0 {
                let window_handle = isize::from(w.hwnd);
                log::debug!("Got window handle: {:?}", icon_handle);
                for icon_type in [ICON_SMALL, ICON_SMALL2, ICON_BIG] {
                    unsafe {
                        SendMessageW(
                            window_handle,
                            WM_SETICON,
                            icon_type.try_into().unwrap(),
                            icon_handle,
                        )
                    };
                }
            }
        }
    }
    State::new(Arc::new(window), frame_pool).await
}
