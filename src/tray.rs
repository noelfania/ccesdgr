use std::mem::size_of;
use std::ptr::{null, null_mut};
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_ERROR, NIIF_INFO, NIM_ADD,
    NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, FindWindowW, GetSystemMetrics, GetWindowRect, PostMessageW, RegisterWindowMessageW, SetForegroundWindow, SetMenuDefaultItem,
    TrackPopupMenu, MF_CHECKED, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, SM_CXSCREEN, SM_CYSCREEN, TPM_BOTTOMALIGN,
    TPM_RETURNCMD, TPM_RIGHTALIGN, TPM_RIGHTBUTTON, WM_APP, WM_LBUTTONDBLCLK, WM_LBUTTONUP,
    WM_NULL, WM_RBUTTONUP,
};

use crate::win;

pub const WM_TRAYICON: u32 = WM_APP + 1;
pub const ID_TRAY_SETTINGS: usize = 1001;
pub const ID_TRAY_EXIT: usize = 1002;
pub const ID_TRAY_RECORD: usize = 1003;
pub const ID_TRAY_SNIP: usize = 1004;
pub const ID_TRAY_DRAW: usize = 1005;
pub const ID_TRAY_TOGGLE_EFFECT: usize = 1006;

const TRAY_UID: u32 = 1;
const APP_TITLE: &str = "GMTL Mouse Click";
static TASKBAR_CREATED: OnceLock<u32> = OnceLock::new();

pub enum TrayAction {
    OpenSettings,
    ShowMenu,
}

pub fn taskbar_created_message() -> u32 {
    *TASKBAR_CREATED.get_or_init(|| {
        let message = win::to_wide("TaskbarCreated");
        unsafe { RegisterWindowMessageW(message.as_ptr()) }
    })
}

pub fn add(hwnd: HWND) {
    let mut data = notify_data(hwnd);
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.uCallbackMessage = WM_TRAYICON;
    data.hIcon = load_tray_icon();

    let tip = if crate::recorder::is_recording() {
        format!("{APP_TITLE} — Recording")
    } else {
        APP_TITLE.to_string()
    };
    copy_wide(&mut data.szTip, &tip);

    unsafe {
        Shell_NotifyIconW(NIM_ADD, &mut data);
    }
}

/// 트레이 툴팁 갱신 (녹화 상태 표시용)
pub fn set_recording_tip(recording: bool) {
    let hwnd = crate::main_hwnd();
    if hwnd.is_null() {
        return;
    }
    let mut data = notify_data(hwnd);
    data.uFlags = NIF_TIP;
    let tip = if recording {
        format!("{APP_TITLE} — Recording")
    } else {
        APP_TITLE.to_string()
    };
    copy_wide(&mut data.szTip, &tip);
    unsafe {
        Shell_NotifyIconW(NIM_MODIFY, &mut data);
    }
}

/// 작업 결과를 트레이 풍선 알림(Windows 알림)으로 표시한다.
pub fn notify(text: &str) {
    show_balloon(text, NIIF_INFO);
}

pub fn notify_error(text: &str) {
    show_balloon(text, NIIF_ERROR);
}

fn show_balloon(text: &str, flags: u32) {
    let hwnd = crate::main_hwnd();
    if hwnd.is_null() {
        return;
    }
    let mut data = notify_data(hwnd);
    data.uFlags = NIF_INFO;
    data.dwInfoFlags = flags;
    copy_wide(&mut data.szInfoTitle, APP_TITLE);
    copy_wide(&mut data.szInfo, text);
    unsafe {
        Shell_NotifyIconW(NIM_MODIFY, &mut data);
    }
}

/// 고정 길이 WCHAR 버퍼에 NUL 종료를 보장하며 복사
fn copy_wide(dest: &mut [u16], text: &str) {
    let wide = win::to_wide(text);
    let len = wide.len().min(dest.len());
    dest[..len].copy_from_slice(&wide[..len]);
    if len == dest.len() && len > 0 {
        dest[len - 1] = 0;
    }
}

fn load_tray_icon() -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
    // 트레이 전용 소형 그림(16~48)이 DPI에 맞게 선택되고, 작업표시줄 밝기에 따라 검정/흰색 선
    win::app_icon(true, !win::taskbar_is_light())
}

/// 테마 변경 후 트레이 아이콘 다시 적용
pub fn refresh_icon(hwnd: HWND) {
    let mut data = notify_data(hwnd);
    data.uFlags = NIF_ICON;
    data.hIcon = load_tray_icon();
    unsafe {
        Shell_NotifyIconW(NIM_MODIFY, &mut data);
    }
}

pub fn remove(hwnd: HWND) {
    let mut data = notify_data(hwnd);
    unsafe {
        Shell_NotifyIconW(NIM_DELETE, &mut data);
    }
}

pub fn action_from_message(message: isize) -> Option<TrayAction> {
    match message as u32 {
        WM_RBUTTONUP => Some(TrayAction::ShowMenu),
        WM_LBUTTONUP | WM_LBUTTONDBLCLK => Some(TrayAction::OpenSettings),
        _ => None,
    }
}

pub fn show_menu(hwnd: HWND) {
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return;
    }

    let settings = win::to_wide("Settings...");
    let record_label = if crate::recorder::is_recording() {
        win::to_wide("Stop GIF Recording\tCtrl+Alt+Shift+R")
    } else {
        win::to_wide("Record GIF...\tCtrl+Alt+Shift+R")
    };
    let snip = win::to_wide("Snapshot...\tCtrl+Alt+Shift+C");
    let draw = win::to_wide("Annotate Screen\tCtrl+Alt+Shift+A");
    let effects = win::to_wide("Click Effects");
    let effects_flag = if crate::click_effects_enabled() {
        MF_CHECKED
    } else {
        MF_UNCHECKED
    };
    let exit = win::to_wide("Exit");

    unsafe {
        AppendMenuW(menu, MF_STRING, ID_TRAY_SETTINGS, settings.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, ID_TRAY_RECORD, record_label.as_ptr());
        AppendMenuW(menu, MF_STRING, ID_TRAY_SNIP, snip.as_ptr());
        AppendMenuW(menu, MF_STRING, ID_TRAY_DRAW, draw.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(
            menu,
            MF_STRING | effects_flag,
            ID_TRAY_TOGGLE_EFFECT,
            effects.as_ptr(),
        );
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, ID_TRAY_EXIT, exit.as_ptr());
        // 좌클릭 동작(Settings)을 굵게 표시
        SetMenuDefaultItem(menu, ID_TRAY_SETTINGS as u32, 0);

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
