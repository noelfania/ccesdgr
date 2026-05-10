use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, FrameRect, InvalidateRect,
    PAINTSTRUCT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetSystemMetrics, LoadCursorW, RegisterClassExW, SetLayeredWindowAttributes, ShowWindow,
    TranslateMessage, CS_HREDRAW, CS_VREDRAW, IDC_CROSS, LWA_ALPHA, MSG, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOW, WM_DESTROY, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WNDCLASSEXW, WS_EX_LAYERED,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use crate::win;

const CLASS_NAME: &str = "GMTLSelectRectangle";
const MIN_SIZE: i32 = 8;

static STATE: Mutex<Option<SelectState>> = Mutex::new(None);

struct SelectState {
    hwnd: isize,
    origin_x: i32,
    origin_y: i32,
    start_x: i32,
    start_y: i32,
    current: RECT,
    dragging: bool,
    result: Option<RECT>,
}

pub fn select_region(owner: HWND) -> Option<RECT> {
    let screen_left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let screen_top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let screen_width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let screen_height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    if screen_width <= 0 || screen_height <= 0 {
        return None;
    }

    let instance = unsafe { GetModuleHandleW(null()) };
    let class_name = win::to_wide(CLASS_NAME);
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(select_proc),
        hInstance: instance,
        hCursor: unsafe { LoadCursorW(null_mut(), IDC_CROSS) },
        lpszClassName: class_name.as_ptr(),
        ..unsafe { zeroed() }
    };
    unsafe {
        RegisterClassExW(&class);
    }

    *STATE.lock().expect("select mutex poisoned") = Some(SelectState {
        hwnd: 0,
        origin_x: screen_left,
        origin_y: screen_top,
        start_x: 0,
        start_y: 0,
        current: RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        dragging: false,
        result: None,
    });

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            class_name.as_ptr(),
            class_name.as_ptr(),
            WS_POPUP,
            screen_left,
            screen_top,
            screen_width,
            screen_height,
            owner,
            null_mut(),
            instance,
            null_mut(),
        )
    };
    if hwnd.is_null() {
        *STATE.lock().expect("select mutex poisoned") = None;
        return None;
    }

    {
        let mut state = STATE.lock().expect("select mutex poisoned");
        if let Some(state) = state.as_mut() {
            state.hwnd = hwnd as isize;
        }
    }

    unsafe {
        SetLayeredWindowAttributes(hwnd, 0, 80, LWA_ALPHA);
        ShowWindow(hwnd, SW_SHOW);
    }

    let mut message: MSG = unsafe { zeroed() };
    loop {
        let active = STATE
            .lock()
            .expect("select mutex poisoned")
            .as_ref()
            .map(|state| state.hwnd != 0)
            .unwrap_or(false);
        if !active {
            break;
        }

        let ok = unsafe { GetMessageW(&mut message, null_mut(), 0, 0) };
        if ok <= 0 {
            break;
        }
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    STATE
        .lock()
        .expect("select mutex poisoned")
        .take()
        .and_then(|state| state.result)
}

unsafe extern "system" fn select_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_LBUTTONDOWN => {
            let x = low_word(lparam);
            let y = high_word(lparam);
            let mut state = STATE.lock().expect("select mutex poisoned");
            if let Some(state) = state.as_mut() {
                state.start_x = x;
                state.start_y = y;
                state.current = RECT {
                    left: x,
                    top: y,
                    right: x,
                    bottom: y,
                };
                state.dragging = true;
            }
            SetCapture(hwnd);
            0
        }
        WM_MOUSEMOVE => {
            let mut state = STATE.lock().expect("select mutex poisoned");
            if let Some(state) = state.as_mut() {
                if state.dragging {
                    state.current =
                        normalize_rect(state.start_x, state.start_y, low_word(lparam), high_word(lparam));
                    InvalidateRect(hwnd, null(), 1);
                }
            }
            0
        }
        WM_LBUTTONUP => {
            ReleaseCapture();
            {
                let mut state = STATE.lock().expect("select mutex poisoned");
                if let Some(state) = state.as_mut() {
                    state.dragging = false;
                    let rect =
                        normalize_rect(state.start_x, state.start_y, low_word(lparam), high_word(lparam));
                    if rect.right - rect.left >= MIN_SIZE && rect.bottom - rect.top >= MIN_SIZE {
                        state.result = Some(RECT {
                            left: state.origin_x + rect.left,
                            top: state.origin_y + rect.top,
                            right: state.origin_x + rect.right,
                            bottom: state.origin_y + rect.bottom,
                        });
                    }
                }
            }
            DestroyWindow(hwnd);
            0
        }
        WM_KEYDOWN => {
            if wparam as i32 == 0x1b {
                DestroyWindow(hwnd);
                return 0;
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_DESTROY => {
            let mut state = STATE.lock().expect("select mutex poisoned");
            if let Some(state) = state.as_mut() {
                state.hwnd = 0;
            }
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn paint(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let brush = CreateSolidBrush(win::colorref(30, 30, 30));
    FillRect(hdc, &ps.rcPaint, brush);
    DeleteObject(brush);

    if let Some(state) = STATE.lock().expect("select mutex poisoned").as_ref() {
        if state.dragging {
            let border = CreateSolidBrush(win::colorref(255, 222, 0));
            FrameRect(hdc, &state.current, border);
            DeleteObject(border);
        }
    }

    EndPaint(hwnd, &ps);
}

fn normalize_rect(x1: i32, y1: i32, x2: i32, y2: i32) -> RECT {
    RECT {
        left: x1.min(x2),
        top: y1.min(y2),
        right: x1.max(x2),
        bottom: y1.max(y2),
    }
}

fn low_word(value: LPARAM) -> i32 {
    (value as u32 & 0xffff) as i16 as i32
}

fn high_word(value: LPARAM) -> i32 {
    ((value as u32 >> 16) & 0xffff) as i16 as i32
}
