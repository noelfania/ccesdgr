use std::mem::size_of;
use std::ptr::{null, null_mut};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreatePen, DeleteObject, Ellipse, EndPaint, FillRect, GetStockObject,
    InvalidateRect, SelectObject, HBRUSH, HDC, NULL_BRUSH, PAINTSTRUCT, PS_SOLID,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetSystemMetrics, KillTimer, RegisterClassExW,
    SetLayeredWindowAttributes, SetTimer, SetWindowPos, ShowWindow, CS_HREDRAW, CS_VREDRAW,
    CW_USEDEFAULT, HWND_TOPMOST, LWA_COLORKEY, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_HIDE, SW_SHOWNA, SWP_NOACTIVATE, SWP_SHOWWINDOW,
    WM_CREATE, WM_DESTROY, WM_PAINT, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::config::ClickColor;
use crate::win;

const CLASS_NAME: &str = "GMTLMouseClickEffectWindow";
const TIMER_ID: usize = 1;
const TIMER_INTERVAL_MS: u32 = 16;
const RIPPLE_LIFETIME: u32 = 28;
const PRESS_RADIUS: i32 = 8;

static RIPPLES: Mutex<Vec<Ripple>> = Mutex::new(Vec::new());
static PRESSES: Mutex<Vec<Press>> = Mutex::new(Vec::new());

#[derive(Clone, Copy)]
struct Ripple {
    x: i32,
    y: i32,
    age: u32,
    start_radius: i32,
    max_radius: i32,
    color: ClickColor,
}

#[derive(Clone, Copy)]
struct Press {
    x: i32,
    y: i32,
    button: usize,
    color: ClickColor,
}

pub fn create_window() -> HWND {
    let instance = unsafe { GetModuleHandleW(null()) };
    let class_name = win::to_wide(CLASS_NAME);

    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(effect_proc),
        hInstance: instance,
        lpszClassName: class_name.as_ptr(),
        ..unsafe { std::mem::zeroed() }
    };

    unsafe {
        RegisterClassExW(&class);
    }

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED
                | WS_EX_TRANSPARENT
                | WS_EX_TOPMOST
                | WS_EX_TOOLWINDOW
                | WS_EX_NOACTIVATE,
            class_name.as_ptr(),
            class_name.as_ptr(),
            WS_POPUP,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1,
            1,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        )
    };

    if !hwnd.is_null() {
        unsafe {
            SetLayeredWindowAttributes(hwnd, win::colorref(0, 0, 0), 0, LWA_COLORKEY);
        }
        unsafe {
            resize_to_virtual_screen(hwnd);
        }
    }

    hwnd
}

pub fn press_down(hwnd: HWND, x: i32, y: i32, button: usize, color: ClickColor) {
    if hwnd.is_null() {
        return;
    }

    {
        let mut presses = PRESSES.lock().expect("effect mutex poisoned");
        presses.retain(|press| press.button != button);
        presses.push(Press {
            x,
            y,
            button,
            color,
        });
    }

    unsafe {
        resize_to_virtual_screen(hwnd);
        ShowWindow(hwnd, SW_SHOWNA);
        SetTimer(hwnd, TIMER_ID, TIMER_INTERVAL_MS, None);
        InvalidateRect(hwnd, null(), 1);
    }
}

pub fn press_up(
    hwnd: HWND,
    x: i32,
    y: i32,
    button: usize,
    color: ClickColor,
    max_radius: i32,
) {
    if hwnd.is_null() {
        return;
    }

    let press = {
        let mut presses = PRESSES.lock().expect("effect mutex poisoned");
        let index = presses.iter().position(|press| press.button == button);
        index.map(|index| presses.remove(index))
    };

    let (color, start_radius) = if let Some(press) = press {
        (press.color, PRESS_RADIUS)
    } else {
        (color, PRESS_RADIUS)
    };

    {
        let mut ripples = RIPPLES.lock().expect("effect mutex poisoned");
        ripples.push(Ripple {
            x,
            y,
            age: 0,
            start_radius,
            max_radius,
            color,
        });
    }

    unsafe {
        resize_to_virtual_screen(hwnd);
        ShowWindow(hwnd, SW_SHOWNA);
        SetTimer(hwnd, TIMER_ID, TIMER_INTERVAL_MS, None);
        InvalidateRect(hwnd, null(), 1);
    }
}

unsafe extern "system" fn effect_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CREATE => 0,
        WM_TIMER => {
            if wparam == TIMER_ID {
                advance(hwnd);
                return 0;
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_DESTROY => {
            KillTimer(hwnd, TIMER_ID);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn advance(hwnd: HWND) {
    let has_ripples = {
        let mut ripples = RIPPLES.lock().expect("effect mutex poisoned");
        for ripple in ripples.iter_mut() {
            ripple.age += 1;
        }
        ripples.retain(|ripple| ripple.age <= RIPPLE_LIFETIME);
        !ripples.is_empty()
    };
    let has_presses = !PRESSES.lock().expect("effect mutex poisoned").is_empty();

    if has_ripples || has_presses {
        InvalidateRect(hwnd, null(), 1);
    } else {
        KillTimer(hwnd, TIMER_ID);
        ShowWindow(hwnd, SW_HIDE);
    }
}

unsafe fn paint(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);

    let black_brush = GetStockObject(windows_sys::Win32::Graphics::Gdi::BLACK_BRUSH);
    FillRect(hdc, &ps.rcPaint, black_brush as HBRUSH);

    let (screen_left, screen_top) = virtual_screen_origin();
    let presses = PRESSES.lock().expect("effect mutex poisoned").clone();
    for press in presses {
        draw_press(hdc, press, screen_left, screen_top);
    }

    let ripples = RIPPLES.lock().expect("effect mutex poisoned").clone();
    for ripple in ripples {
        draw_ripple(hdc, ripple, screen_left, screen_top);
    }

    EndPaint(hwnd, &ps);
}

unsafe fn draw_press(hdc: HDC, press: Press, screen_left: i32, screen_top: i32) {
    let pen = CreatePen(PS_SOLID, 3, press.color.to_colorref());
    let old_pen = SelectObject(hdc, pen);
    let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH));

    let x = press.x - screen_left;
    let y = press.y - screen_top;
    Ellipse(
        hdc,
        x - PRESS_RADIUS,
        y - PRESS_RADIUS,
        x + PRESS_RADIUS,
        y + PRESS_RADIUS,
    );

    SelectObject(hdc, old_brush);
    SelectObject(hdc, old_pen);
    DeleteObject(pen);
}

unsafe fn draw_ripple(hdc: HDC, ripple: Ripple, screen_left: i32, screen_top: i32) {
    let progress = ripple.age as f32 / RIPPLE_LIFETIME as f32;
    let radius = (ripple.start_radius as f32
        + (ripple.max_radius - ripple.start_radius).max(0) as f32 * progress) as i32;
    let thickness = (4.0 - progress * 2.5).max(1.0) as i32;

    let pen = CreatePen(PS_SOLID, thickness, ripple.color.to_colorref());
    let old_pen = SelectObject(hdc, pen);
    let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH));

    let x = ripple.x - screen_left;
    let y = ripple.y - screen_top;
    Ellipse(hdc, x - radius, y - radius, x + radius, y + radius);

    SelectObject(hdc, old_brush);
    SelectObject(hdc, old_pen);
    DeleteObject(pen);
}

unsafe fn resize_to_virtual_screen(hwnd: HWND) {
    let (left, top) = virtual_screen_origin();
    let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
    let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
    SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        left,
        top,
        width,
        height,
        SWP_NOACTIVATE | SWP_SHOWWINDOW,
    );
}

unsafe fn virtual_screen_origin() -> (i32, i32) {
    (
        GetSystemMetrics(SM_XVIRTUALSCREEN),
        GetSystemMetrics(SM_YVIRTUALSCREEN),
    )
}
