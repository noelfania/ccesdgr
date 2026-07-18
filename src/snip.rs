use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject, SRCCOPY,
};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

use crate::select_rect;
use crate::win;

const CF_BITMAP_FORMAT: u32 = 2;

pub fn run(owner: HWND) {
    let Some(rect) = select_rect::select_region(owner) else {
        return;
    };

    if unsafe { copy_region_to_clipboard(owner, rect) }.is_err() {
        let title = win::to_wide("GMTL Mouse Click");
        let message = win::to_wide("Failed to copy the snapshot to the clipboard.");
        unsafe {
            MessageBoxW(owner, message.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR);
        }
    }
}

unsafe fn copy_region_to_clipboard(owner: HWND, rect: RECT) -> Result<(), ()> {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width <= 0 || height <= 0 {
        return Err(());
    }

    let screen_dc = GetDC(null_mut());
    if screen_dc.is_null() {
        return Err(());
    }

    let mem_dc = CreateCompatibleDC(screen_dc);
    if mem_dc.is_null() {
        ReleaseDC(null_mut(), screen_dc);
        return Err(());
    }

    let bitmap = CreateCompatibleBitmap(screen_dc, width, height);
    if bitmap.is_null() {
        DeleteDC(mem_dc);
        ReleaseDC(null_mut(), screen_dc);
        return Err(());
    }

    let old = SelectObject(mem_dc, bitmap);
    let copied = BitBlt(mem_dc, 0, 0, width, height, screen_dc, rect.left, rect.top, SRCCOPY) != 0;
    SelectObject(mem_dc, old);
    DeleteDC(mem_dc);
    ReleaseDC(null_mut(), screen_dc);
    if !copied {
        DeleteObject(bitmap);
        return Err(());
    }

    if OpenClipboard(owner) == 0 {
        DeleteObject(bitmap);
        return Err(());
    }
    EmptyClipboard();
    let handle = SetClipboardData(CF_BITMAP_FORMAT, bitmap);
    CloseClipboard();

    if handle.is_null() {
        DeleteObject(bitmap);
        Err(())
    } else {
        // 성공 시 비트맵 핸들의 소유권은 클립보드로 넘어간다.
        Ok(())
    }
}
