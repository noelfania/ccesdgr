use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CombineRgn, CreateRectRgn, CreateSolidBrush, DeleteObject, EndPaint, FillRect,
    InvalidateRect, PAINTSTRUCT, RGN_DIFF, SetWindowRgn,
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
const BORDER: i32 = 1; // 1px 헤어라인 (capture_ui와 동일)
const STRIPE_LEN: i32 = 4; // 흰/검 대시 길이

static STATE: Mutex<Option<SelectState>> = Mutex::new(None);

struct SelectState {
    hwnd: isize,
    origin_x: i32,
    origin_y: i32,
    screen_w: i32,
    screen_h: i32,
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

    // 선택 중 단축키를 다시 눌러도 첫 오버레이 상태를 덮어쓰지 않는다.
    if STATE.lock().expect("select mutex poisoned").is_some() {
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
        screen_w: screen_width,
        screen_h: screen_height,
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
        // 바깥만 반투명 딤; 선택 안쪽은 SetWindowRgn 구멍으로 완전 투명
        SetLayeredWindowAttributes(hwnd, 0, 100, LWA_ALPHA);
        apply_select_region(hwnd, None, screen_width, screen_height);
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
            let update = {
                let mut state = STATE.lock().expect("select mutex poisoned");
                state.as_mut().and_then(|state| {
                    if !state.dragging {
                        return None;
                    }
                    state.current = normalize_rect(
                        state.start_x,
                        state.start_y,
                        low_word(lparam),
                        high_word(lparam),
                    );
                    Some((state.current, state.screen_w, state.screen_h))
                })
            };
            if let Some((sel, sw, sh)) = update {
                apply_select_region(hwnd, Some(sel), sw, sh);
                InvalidateRect(hwnd, null(), 1);
            }
            0
        }
        WM_LBUTTONUP => {
            // 오버레이가 뜨기 전에 누른 버튼을 뗀 경우는 무시 (원점부터 잘못 선택 방지)
            let dragging = STATE
                .lock()
                .expect("select mutex poisoned")
                .as_ref()
                .is_some_and(|state| state.dragging);
            if !dragging {
                return 0;
            }
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

/// 전체 화면에서 선택 안쪽을 빼 구멍 냄 — 테두리(BORDER) 링은 남겨 줄무늬를 그림.
unsafe fn apply_select_region(hwnd: HWND, sel: Option<RECT>, screen_w: i32, screen_h: i32) {
    let full = CreateRectRgn(0, 0, screen_w, screen_h);
    if let Some(sel) = sel {
        let inner_l = sel.left + BORDER;
        let inner_t = sel.top + BORDER;
        let inner_r = sel.right - BORDER;
        let inner_b = sel.bottom - BORDER;
        if inner_r > inner_l && inner_b > inner_t {
            let hole = CreateRectRgn(inner_l, inner_t, inner_r, inner_b);
            CombineRgn(full, full, hole, RGN_DIFF);
            DeleteObject(hole as _);
        }
    }
    // SetWindowRgn이 소유권을 가져감
    SetWindowRgn(hwnd, full, 1);
}

unsafe fn paint(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let brush = CreateSolidBrush(win::colorref(20, 20, 20));
    FillRect(hdc, &ps.rcPaint, brush);
    DeleteObject(brush as _);

    if let Some(state) = STATE.lock().expect("select mutex poisoned").as_ref() {
        if state.dragging {
            let sel = state.current;
            let sel_w = sel.right - sel.left;
            let sel_h = sel.bottom - sel.top;
            if sel_w > 0 && sel_h > 0 {
                paint_stripe_border(
                    hdc,
                    sel.left,
                    sel.top,
                    sel_w,
                    sel_h,
                    BORDER,
                    STRIPE_LEN,
                    win::colorref(255, 255, 255),
                    win::colorref(0, 0, 0),
                );
            }
        }
    }

    EndPaint(hwnd, &ps);
}

/// capture_ui와 동일한 흰/검(대시) 테두리.
unsafe fn paint_stripe_border(
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    fx: i32,
    fy: i32,
    sel_w: i32,
    sel_h: i32,
    thickness: i32,
    dash_len: i32,
    color_a: COLORREF,
    color_b: COLORREF,
) {
    let t = thickness.max(1);
    let dash = dash_len.max(1);
    paint_stripe_edge(hdc, fx, fy, fx + sel_w, fy + t, true, dash, color_a, color_b);
    paint_stripe_edge(
        hdc,
        fx,
        fy + sel_h - t,
        fx + sel_w,
        fy + sel_h,
        true,
        dash,
        color_a,
        color_b,
    );
    paint_stripe_edge(
        hdc,
        fx,
        fy + t,
        fx + t,
        fy + sel_h - t,
        false,
        dash,
        color_a,
        color_b,
    );
    paint_stripe_edge(
        hdc,
        fx + sel_w - t,
        fy + t,
        fx + sel_w,
        fy + sel_h - t,
        false,
        dash,
        color_a,
        color_b,
    );
}

unsafe fn paint_stripe_edge(
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    horizontal: bool,
    dash: i32,
    color_a: COLORREF,
    color_b: COLORREF,
) {
    let width = right - left;
    let height = bottom - top;
    if width <= 0 || height <= 0 {
        return;
    }

    if horizontal {
        let mut x = left;
        let mut idx = 0;
        while x < right {
            let w = dash.min(right - x);
            let color = if idx % 2 == 0 { color_a } else { color_b };
            let brush = CreateSolidBrush(color);
            FillRect(
                hdc,
                &RECT {
                    left: x,
                    top,
                    right: x + w,
                    bottom,
                },
                brush,
            );
            DeleteObject(brush as _);
            x += w;
            idx += 1;
        }
    } else {
        let mut y = top;
        let mut idx = 0;
        while y < bottom {
            let h = dash.min(bottom - y);
            let color = if idx % 2 == 0 { color_a } else { color_b };
            let brush = CreateSolidBrush(color);
            FillRect(
                hdc,
                &RECT {
                    left,
                    top: y,
                    right,
                    bottom: y + h,
                },
                brush,
            );
            DeleteObject(brush as _);
            y += h;
            idx += 1;
        }
    }
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
