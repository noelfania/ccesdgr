use std::mem::size_of;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, FindWindowW, GetSystemMetrics, GetWindowRect,
    LoadIconW, PostMessageW, SetForegroundWindow, TrackPopupMenu, IDI_APPLICATION, MF_SEPARATOR,
    MF_STRING, SM_CXSCREEN, SM_CYSCREEN, TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTALIGN,
    TPM_RIGHTBUTTON, WM_APP, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP,
};

use crate::win;

pub const WM_TRAYICON: u32 = WM_APP + 1;
pub const ID_TRAY_SETTINGS: usize = 1001;
pub const ID_TRAY_EXIT: usize = 1002;
pub const ID_TRAY_RECORD: usize = 1003;

const TRAY_UID: u32 = 1;

pub fn add(hwnd: HWND) {
    let mut data = notify_data(hwnd);
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.uCallbackMessage = WM_TRAYICON;
    data.hIcon = load_app_icon();

    let tip = win::to_wide("GMTL Mouse Click");
    for (index, value) in tip.iter().take(data.szTip.len()).enumerate() {
        data.szTip[index] = *value;
    }

    unsafe {
        Shell_NotifyIconW(NIM_ADD, &mut data);
    }
}

fn load_app_icon() -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
    let instance = unsafe { GetModuleHandleW(null()) };
    let icon = unsafe { LoadIconW(instance, win::resource_id(win::APP_ICON_ID)) };
    if icon.is_null() {
        unsafe { LoadIconW(null_mut(), IDI_APPLICATION) }
    } else {
        icon
    }
}

pub fn remove(hwnd: HWND) {
    let mut data = notify_data(hwnd);
    unsafe {
        Shell_NotifyIconW(NIM_DELETE, &mut data);
    }
}

pub fn is_activate_message(message: isize) -> bool {
    message as u32 == WM_RBUTTONUP || message as u32 == WM_LBUTTONUP
}

pub fn show_menu(hwnd: HWND) {
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return;
    }

    let settings = win::to_wide("설정...");
    let record_label = if crate::recorder::is_recording() {
        win::to_wide("GIF 녹화 중지")
    } else {
        win::to_wide("GIF 녹화...")
    };
    let exit = win::to_wide("종료");

    unsafe {
        AppendMenuW(menu, MF_STRING, ID_TRAY_SETTINGS, settings.as_ptr());
        AppendMenuW(menu, MF_STRING, ID_TRAY_RECORD, record_label.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, ID_TRAY_EXIT, exit.as_ptr());

        let mut point = POINT { x: 0, y: 0 };
        GetCursorPos(&mut point);

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let menu_x = point.x;
        let mut menu_y = point.y;
        let taskbar_rect = taskbar_rect();
        let mut flags = TPM_RIGHTBUTTON | TPM_RETURNCMD;
        if point.y > screen_h / 2 {
            flags |= TPM_BOTTOMALIGN;
            if let Some(taskbar) = taskbar_rect {
                if point.y >= taskbar.top && point.y <= taskbar.bottom {
                    menu_y = taskbar.top - 1;
                }
            }
        }
        if point.x > screen_w / 2 {
            flags |= TPM_RIGHTALIGN;
        }

        SetForegroundWindow(hwnd);
        let command = TrackPopupMenu(menu, flags, menu_x, menu_y, 0, hwnd, null_mut());

        if command != 0 {
            PostMessageW(
                hwnd,
                windows_sys::Win32::UI::WindowsAndMessaging::WM_COMMAND,
                command as usize,
                0,
            );
        }

        DestroyMenu(menu);
        PostMessageW(hwnd, WM_NULL, 0, 0);
    }
}

unsafe fn taskbar_rect() -> Option<RECT> {
    let class_name = win::to_wide("Shell_TrayWnd");
    let hwnd = FindWindowW(class_name.as_ptr(), null());
    if hwnd.is_null() {
        return None;
    }

    let mut rect: RECT = std::mem::zeroed();
    if GetWindowRect(hwnd, &mut rect) == 0 {
        None
    } else {
        Some(rect)
    }
}

fn notify_data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_UID,
        ..unsafe { std::mem::zeroed() }
    }
}
