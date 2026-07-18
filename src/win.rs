use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null;

use windows_sys::Win32::Foundation::{COLORREF, HINSTANCE, RECT};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_CXSCREEN, SM_CYSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

/// exe·트레이 공용 아이콘 (assets/icon-entry-3.png → build.rs ICO)
pub const APP_ICON_ID: usize = 1;

pub fn module_handle() -> HINSTANCE {
    unsafe { GetModuleHandleW(null()) }
}

pub fn to_wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}

pub fn colorref(red: u8, green: u8, blue: u8) -> COLORREF {
    red as COLORREF | ((green as COLORREF) << 8) | ((blue as COLORREF) << 16)
}

pub fn low_word(value: usize) -> u16 {
    (value & 0xffff) as u16
}

pub fn resource_id(id: usize) -> *const u16 {
    id as *const u16
}

pub fn virtual_screen_rect() -> RECT {
    unsafe {
        let left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let top = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        RECT {
            left,
            top,
            right: left + width,
            bottom: top + height,
        }
    }
}

pub fn primary_screen_rect() -> RECT {
    unsafe {
        let width = GetSystemMetrics(SM_CXSCREEN);
        let height = GetSystemMetrics(SM_CYSCREEN);
        RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        }
    }
}

pub fn rect_width(rect: &RECT) -> i32 {
    rect.right - rect.left
}

pub fn rect_height(rect: &RECT) -> i32 {
    rect.bottom - rect.top
}

pub fn intersect_rect(a: RECT, b: RECT) -> Option<RECT> {
    let left = a.left.max(b.left);
    let top = a.top.max(b.top);
    let right = a.right.min(b.right);
    let bottom = a.bottom.min(b.bottom);
    if right > left && bottom > top {
        Some(RECT {
            left,
            top,
            right,
            bottom,
        })
    } else {
        None
    }
}

/// rect를 가상 화면 안으로 잘라낸다. 교집합이 있으면 true.
pub fn clamp_rect_to_virtual_screen(rect: &mut RECT) -> bool {
    let screen = virtual_screen_rect();
    if let Some(inter) = intersect_rect(*rect, screen) {
        *rect = inter;
        true
    } else {
        false
    }
}
