#![windows_subsystem = "windows"]

mod capture_ui;
mod config;
mod draw;
mod effect;
mod recorder;
mod select_rect;
mod settings;
mod snip;
mod startup;
mod tray;
mod win;

use std::mem::size_of;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{
    InitCommonControlsEx, ICC_STANDARD_CLASSES, ICC_WIN95_CLASSES, INITCOMMONCONTROLSEX,
};
use windows_sys::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    LoadIconW, MessageBoxW, PostMessageW, PostQuitMessage, RegisterClassExW, SetProcessDPIAware,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, CS_HREDRAW, CS_VREDRAW,
    CW_USEDEFAULT, HHOOK, MB_ICONERROR, MB_OK, MSG, MSLLHOOKSTRUCT, WH_MOUSE_LL, WM_APP,
    WM_COMMAND, WM_CREATE, WM_DESTROY, WM_HOTKEY, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_RBUTTONDOWN,
    WM_RBUTTONUP, WNDCLASSEXW, WS_OVERLAPPED,
};

use crate::config::{AppConfig, CaptureFrameConfig};

const MAIN_CLASS_NAME: &str = "GMTLMouseClickMainWindow";
const WM_MOUSE_CLICKED: u32 = WM_APP + 2;
const HOTKEY_SNIP: i32 = 1;
const HOTKEY_DRAW: i32 = 2;
const VK_C: u32 = 0x43;
const VK_A: u32 = 0x41;
const BUTTON_LEFT: usize = 1;
const BUTTON_RIGHT: usize = 2;

static APP_STATE: OnceLock<Mutex<AppState>> = OnceLock::new();
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);

struct AppState {
    config: AppConfig,
    effect_hwnd: isize,
    mouse_hook: isize,
}

struct MouseClick {
    x: i32,
    y: i32,
    button: usize,
    phase: MousePhase,
}

#[derive(Clone, Copy)]
enum MousePhase {
    Down,
    Up,
}

fn main() {
    enable_dpi_awareness();
    init_common_controls();

    let config = AppConfig::load();
    let instance = unsafe { GetModuleHandleW(null()) };
    let hwnd = create_main_window(instance);

    if hwnd.is_null() {
        show_error(null_mut(), "Failed to create the main window.");
        return;
    }

    let effect_hwnd = effect::create_window();
    let mouse_hook = install_mouse_hook(instance);
    if mouse_hook.is_null() {
        show_error(hwnd, "Failed to register the mouse hook.");
    }

    if !register_hotkeys(hwnd) {
        show_error(hwnd, "Failed to register global hotkeys.");
    }

    APP_STATE
        .set(Mutex::new(AppState {
            config,
            effect_hwnd: effect_hwnd as isize,
            mouse_hook: mouse_hook as isize,
        }))
        .ok();

    tray::add(hwnd);
    tray::taskbar_created_message();
    message_loop();
}

fn enable_dpi_awareness() {
    unsafe {
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) == 0 {
            SetProcessDPIAware();
        }
    }
}

/// ComCtl32 v6 테마 컨트롤 등록 (매니페스트 + InitCommonControlsEx)
fn init_common_controls() {
    let icc = INITCOMMONCONTROLSEX {
        dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_WIN95_CLASSES | ICC_STANDARD_CLASSES,
    };
    unsafe {
        InitCommonControlsEx(&icc);
    }
}

pub fn save_capture_frame(frame: CaptureFrameConfig) {
    if let Some(state) = APP_STATE.get() {
        let mut state = state.lock().expect("app state mutex poisoned");
        state.config.capture_frame = Some(frame);
        let _ = state.config.save();
    }
}

pub fn apply_settings(config: AppConfig) -> Result<(), String> {
    startup::set_enabled(config.start_with_windows)
        .map_err(|error| format!("Failed to save startup setting: {error}"))?;
    config
        .save()
        .map_err(|error| format!("Failed to save config file: {error}"))?;

    let mut config = config;
    config.clamp_values();

    if let Some(state) = APP_STATE.get() {
        state.lock().expect("app state mutex poisoned").config = config;
    }

    Ok(())
}

fn register_hotkeys(hwnd: HWND) -> bool {
    let modifiers = MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT;
    unsafe {
        RegisterHotKey(hwnd, HOTKEY_SNIP, modifiers, VK_C) != 0
            && RegisterHotKey(hwnd, HOTKEY_DRAW, modifiers, VK_A) != 0
    }
}

fn unregister_hotkeys(hwnd: HWND) {
    unsafe {
        UnregisterHotKey(hwnd, HOTKEY_SNIP);
        UnregisterHotKey(hwnd, HOTKEY_DRAW);
    }
}

fn create_main_window(instance: HINSTANCE) -> HWND {
    let class_name = win::to_wide(MAIN_CLASS_NAME);
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(main_proc),
        hInstance: instance,
        hIcon: unsafe { LoadIconW(instance, win::resource_id(win::APP_ICON_ID)) },
        hIconSm: unsafe { LoadIconW(instance, win::resource_id(win::APP_ICON_ID)) },
        lpszClassName: class_name.as_ptr(),
        ..unsafe { std::mem::zeroed() }
    };

    unsafe {
        RegisterClassExW(&class);
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            class_name.as_ptr(),
            WS_OVERLAPPED,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        )
    }
}

fn install_mouse_hook(instance: HINSTANCE) -> HHOOK {
    unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), instance, 0) }
}

fn message_loop() {
    let mut message: MSG = unsafe { std::mem::zeroed() };
    unsafe {
        while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

unsafe extern "system" fn main_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CREATE => {
            MAIN_HWND.store(hwnd as isize, Ordering::Relaxed);
            0
        }
        tray::WM_TRAYICON => {
            if let Some(action) = tray::action_from_message(lparam) {
                match action {
                    tray::TrayAction::OpenSettings => open_settings(hwnd),
                    tray::TrayAction::ShowMenu => tray::show_menu(hwnd),
                }
            }
            0
        }
        WM_COMMAND => {
            match wparam & 0xffff {
                tray::ID_TRAY_SETTINGS => open_settings(hwnd),
                tray::ID_TRAY_RECORD => open_record(hwnd),
                tray::ID_TRAY_SNIP => snip::run(hwnd),
                tray::ID_TRAY_DRAW => draw::show(),
                tray::ID_TRAY_EXIT => {
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
            0
        }
        WM_MOUSE_CLICKED => {
            handle_mouse_click(lparam);
            0
        }
        WM_HOTKEY => {
            match wparam as i32 {
                HOTKEY_SNIP => snip::run(hwnd),
                HOTKEY_DRAW => draw::show(),
                _ => {}
            }
            0
        }
        WM_DESTROY => {
            cleanup(hwnd);
            PostQuitMessage(0);
            0
        }
        _ if message == tray::taskbar_created_message() => {
            tray::add(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let message = wparam as u32;
    if code >= 0
        && (message == WM_LBUTTONDOWN
            || message == WM_RBUTTONDOWN
            || message == WM_LBUTTONUP
            || message == WM_RBUTTONUP)
    {
        let hwnd = MAIN_HWND.load(Ordering::Relaxed) as HWND;
        if !hwnd.is_null() {
            let info = *(lparam as *const MSLLHOOKSTRUCT);
            let button = if message == WM_LBUTTONDOWN || message == WM_LBUTTONUP {
                BUTTON_LEFT
            } else {
                BUTTON_RIGHT
            };
            let phase = if message == WM_LBUTTONDOWN || message == WM_RBUTTONDOWN {
                MousePhase::Down
            } else {
                MousePhase::Up
            };
            let click = Box::new(MouseClick {
                x: info.pt.x,
                y: info.pt.y,
                button,
                phase,
            });
            let raw_click = Box::into_raw(click);
            if PostMessageW(hwnd, WM_MOUSE_CLICKED, 0, raw_click as LPARAM) == 0 {
                let _ = Box::from_raw(raw_click);
            }
        }
    }

    CallNextHookEx(null_mut(), code, wparam, lparam)
}

unsafe fn handle_mouse_click(lparam: LPARAM) {
    if lparam == 0 {
        return;
    }

    let click = Box::from_raw(lparam as *mut MouseClick);
    if draw::is_active() {
        return;
    }

    let Some(state) = APP_STATE.get() else {
        return;
    };

    let state = state.lock().expect("app state mutex poisoned");
    if !state.config.click_effect_enabled {
        return;
    }

    let color = if click.button == BUTTON_LEFT {
        state.config.left_click_color
    } else {
        state.config.right_click_color
    };

    let lifetime = state.config.ripple_lifetime;
    let radius = state.config.ripple_radius;
    let effect_kind = state.config.click_effect_kind;
    match click.phase {
        MousePhase::Down => effect::press_down(
            state.effect_hwnd as HWND,
            click.x,
            click.y,
            click.button,
            color,
            radius,
            effect_kind,
        ),
        MousePhase::Up => effect::press_up(
            state.effect_hwnd as HWND,
            click.x,
            click.y,
            click.button,
            color,
            radius,
            lifetime,
            effect_kind,
        ),
    }
}

unsafe fn open_settings(hwnd: HWND) {
    let Some(state) = APP_STATE.get() else {
        return;
    };
    let config = state
        .lock()
        .expect("app state mutex poisoned")
        .config
        .clone();
    settings::show(hwnd, &config);
}

unsafe fn open_record(hwnd: HWND) {
    // 녹화 중이면 중지
    if recorder::is_recording() {
        recorder::stop();
        return;
    }
    let Some(state) = APP_STATE.get() else {
        return;
    };
    let config = state
        .lock()
        .expect("app state mutex poisoned")
        .config
        .clone();
    capture_ui::show(hwnd, config);
}

unsafe fn cleanup(hwnd: HWND) {
    unregister_hotkeys(hwnd);
    tray::remove(hwnd);
    capture_ui::close();
    recorder::stop();
    MAIN_HWND.store(0, Ordering::Relaxed);

    if let Some(state) = APP_STATE.get() {
        let state = state.lock().expect("app state mutex poisoned");
        let mouse_hook = state.mouse_hook as HHOOK;
        let effect_hwnd = state.effect_hwnd as HWND;
        if !mouse_hook.is_null() {
            UnhookWindowsHookEx(mouse_hook);
        }
        if !effect_hwnd.is_null() {
            DestroyWindow(effect_hwnd);
        }
    }
}

fn show_error(hwnd: HWND, message: &str) {
    let title = win::to_wide("GMTL Mouse Click");
    let message = win::to_wide(message);
    unsafe {
        MessageBoxW(hwnd, message.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR);
    }
}
