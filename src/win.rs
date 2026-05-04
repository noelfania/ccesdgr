use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null;

use windows_sys::Win32::Foundation::{COLORREF, HINSTANCE};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

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
