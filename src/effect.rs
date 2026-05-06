use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::{null, null_mut};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, AC_SRC_ALPHA,
    AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetSystemMetrics, KillTimer, RegisterClassExW, SetTimer,
    SetWindowPos, ShowWindow, UpdateLayeredWindow, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT,
    HWND_TOPMOST, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNA, ULW_ALPHA, WM_CREATE, WM_DESTROY, WM_TIMER,
    WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::config::{ClickColor, ClickEffectKind};
use crate::win;

const CLASS_NAME: &str = "GMTLMouseClickEffectWindow";
const TIMER_ID: usize = 1;
const TIMER_INTERVAL_MS: u32 = 16;
const PRESS_RADIUS: i32 = 8;

static RIPPLES: Mutex<Vec<Ripple>> = Mutex::new(Vec::new());
static PRESSES: Mutex<Vec<Press>> = Mutex::new(Vec::new());
static FADES: Mutex<Vec<Fade>> = Mutex::new(Vec::new());

#[derive(Clone, Copy)]
struct Ripple {
    x: i32,
    y: i32,
    age: u32,
    start_radius: i32,
    max_radius: i32,
    color: ClickColor,
    lifetime: u32,
}

#[derive(Clone, Copy)]
struct Press {
    x: i32,
    y: i32,
    button: usize,
    color: ClickColor,
    radius: i32,
    effect_kind: ClickEffectKind,
}

#[derive(Clone, Copy)]
struct Fade {
    x: i32,
    y: i32,
    age: u32,
    lifetime: u32,
    radius: i32,
    color: ClickColor,
    effect_kind: ClickEffectKind,
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
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
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
            resize_to_virtual_screen(hwnd);
        }
    }

    hwnd
}

pub fn press_down(
    hwnd: HWND,
    x: i32,
    y: i32,
    button: usize,
    color: ClickColor,
    _radius: i32,
    effect_kind: ClickEffectKind,
) {
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
            radius: PRESS_RADIUS,
            effect_kind,
        });
    }

    unsafe {
        resize_to_virtual_screen(hwnd);
        ShowWindow(hwnd, SW_SHOWNA);
        SetTimer(hwnd, TIMER_ID, TIMER_INTERVAL_MS, None);
        render_effect_window(hwnd);
    }
}

pub fn press_up(
    hwnd: HWND,
    x: i32,
    y: i32,
    button: usize,
    color: ClickColor,
    max_radius: i32,
    lifetime: u32,
    effect_kind: ClickEffectKind,
) {
    if hwnd.is_null() {
        return;
    }

    let press = {
        let mut presses = PRESSES.lock().expect("effect mutex poisoned");
        let index = presses.iter().position(|press| press.button == button);
        index.map(|index| presses.remove(index))
    };

    let (color, effect_kind, start_radius) = if let Some(press) = press {
        (press.color, press.effect_kind, press.radius)
    } else {
        (color, effect_kind, PRESS_RADIUS)
    };

    match effect_kind {
        ClickEffectKind::Ripple => {
            let mut ripples = RIPPLES.lock().expect("effect mutex poisoned");
            ripples.push(Ripple {
                x,
                y,
                age: 0,
                start_radius,
                max_radius,
                color,
                lifetime: lifetime.max(1),
            });
        }
        ClickEffectKind::FilledFade | ClickEffectKind::FilledCrossFade => {
            let mut fades = FADES.lock().expect("effect mutex poisoned");
            fades.push(Fade {
                x,
                y,
                age: 0,
                lifetime: lifetime.max(1),
                radius: start_radius,
                color,
                effect_kind,
            });
        }
    }

    unsafe {
        resize_to_virtual_screen(hwnd);
        ShowWindow(hwnd, SW_SHOWNA);
        SetTimer(hwnd, TIMER_ID, TIMER_INTERVAL_MS, None);
        render_effect_window(hwnd);
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
        ripples.retain(|ripple| ripple.age <= ripple.lifetime);
        !ripples.is_empty()
    };
    let has_fades = {
        let mut fades = FADES.lock().expect("effect mutex poisoned");
        for fade in fades.iter_mut() {
            fade.age += 1;
        }
        fades.retain(|fade| fade.age <= fade.lifetime);
        !fades.is_empty()
    };
    let has_presses = !PRESSES.lock().expect("effect mutex poisoned").is_empty();

    if has_ripples || has_fades || has_presses {
        render_effect_window(hwnd);
    } else {
        KillTimer(hwnd, TIMER_ID);
        ShowWindow(hwnd, SW_HIDE);
    }
}

unsafe fn render_effect_window(hwnd: HWND) {
    let (screen_left, screen_top) = virtual_screen_origin();
    let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
    let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
    if width <= 0 || height <= 0 {
        return;
    }

    let mut buffer = vec![0u8; width as usize * height as usize * 4];

    let presses = PRESSES.lock().expect("effect mutex poisoned").clone();
    for press in presses {
        draw_press(&mut buffer, width, height, press, screen_left, screen_top);
    }

    let ripples = RIPPLES.lock().expect("effect mutex poisoned").clone();
    for ripple in ripples {
        draw_ripple(&mut buffer, width, height, ripple, screen_left, screen_top);
    }

    let fades = FADES.lock().expect("effect mutex poisoned").clone();
    for fade in fades {
        draw_fade(&mut buffer, width, height, fade, screen_left, screen_top);
    }

    update_layered_window(hwnd, screen_left, screen_top, width, height, &buffer);
}

fn draw_press(
    buffer: &mut [u8],
    width: i32,
    height: i32,
    press: Press,
    screen_left: i32,
    screen_top: i32,
) {
    let x = press.x - screen_left;
    let y = press.y - screen_top;
    match press.effect_kind {
        ClickEffectKind::Ripple => draw_ring(
            buffer,
            width,
            height,
            x,
            y,
            press.radius,
            3,
            press.color,
            255,
        ),
        ClickEffectKind::FilledFade => {
            draw_filled_circle(buffer, width, height, x, y, press.radius, press.color, 255)
        }
        ClickEffectKind::FilledCrossFade => draw_filled_circle_with_cross_cutout(
            buffer,
            width,
            height,
            x,
            y,
            press.radius,
            press.color,
            255,
        ),
    }
}

fn draw_ripple(
    buffer: &mut [u8],
    width: i32,
    height: i32,
    ripple: Ripple,
    screen_left: i32,
    screen_top: i32,
) {
    let progress = ripple.age as f32 / ripple.lifetime as f32;
    let radius = (ripple.start_radius as f32
        + (ripple.max_radius - ripple.start_radius).max(0) as f32 * progress)
        as i32;
    let thickness = (4.0 - progress * 2.5).max(1.0) as i32;

    let x = ripple.x - screen_left;
    let y = ripple.y - screen_top;
    draw_ring(
        buffer,
        width,
        height,
        x,
        y,
        radius,
        thickness,
        ripple.color,
        255,
    );
}

fn draw_fade(
    buffer: &mut [u8],
    width: i32,
    height: i32,
    fade: Fade,
    screen_left: i32,
    screen_top: i32,
) {
    let progress = (fade.age as f32 / fade.lifetime as f32).clamp(0.0, 1.0);
    let alpha = (255.0 * (1.0 - progress)).round() as u8;
    if alpha == 0 {
        return;
    }

    let x = fade.x - screen_left;
    let y = fade.y - screen_top;
    match fade.effect_kind {
        ClickEffectKind::FilledCrossFade => {
            draw_filled_circle_with_cross_cutout(
                buffer,
                width,
                height,
                x,
                y,
                fade.radius,
                fade.color,
                alpha,
            );
        }
        _ => {
            draw_filled_circle(buffer, width, height, x, y, fade.radius, fade.color, alpha);
        }
    }
}

fn draw_ring(
    buffer: &mut [u8],
    width: i32,
    height: i32,
    cx: i32,
    cy: i32,
    radius: i32,
    thickness: i32,
    color: ClickColor,
    alpha: u8,
) {
    let radius = radius.max(1);
    let half = (thickness.max(1) as f32) / 2.0;
    let min_dist = (radius as f32 - half).max(0.0);
    let max_dist = radius as f32 + half;
    let min_sq = min_dist * min_dist;
    let max_sq = max_dist * max_dist;
    let left = (cx - radius - thickness).max(0);
    let top = (cy - radius - thickness).max(0);
    let right = (cx + radius + thickness).min(width - 1);
    let bottom = (cy + radius + thickness).min(height - 1);

    for py in top..=bottom {
        for px in left..=right {
            let dx = (px - cx) as f32;
            let dy = (py - cy) as f32;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq >= min_sq && dist_sq <= max_sq {
                blend_pixel(buffer, width, px, py, color, alpha);
            }
        }
    }
}

fn draw_filled_circle(
    buffer: &mut [u8],
    width: i32,
    height: i32,
    cx: i32,
    cy: i32,
    radius: i32,
    color: ClickColor,
    alpha: u8,
) {
    let radius = radius.max(1);
    let radius_sq = radius * radius;
    let left = (cx - radius).max(0);
    let top = (cy - radius).max(0);
    let right = (cx + radius).min(width - 1);
    let bottom = (cy + radius).min(height - 1);

    for py in top..=bottom {
        for px in left..=right {
            let dx = px - cx;
            let dy = py - cy;
            if dx * dx + dy * dy <= radius_sq {
                blend_pixel(buffer, width, px, py, color, alpha);
            }
        }
    }
}

fn draw_filled_circle_with_cross_cutout(
    buffer: &mut [u8],
    width: i32,
    height: i32,
    cx: i32,
    cy: i32,
    radius: i32,
    color: ClickColor,
    alpha: u8,
) {
    let radius = radius.max(1);
    let radius_sq = radius * radius;
    let left = (cx - radius).max(0);
    let top = (cy - radius).max(0);
    let right = (cx + radius).min(width - 1);
    let bottom = (cy + radius).min(height - 1);

    for py in top..=bottom {
        for px in left..=right {
            let dx = px - cx;
            let dy = py - cy;
            if dx * dx + dy * dy <= radius_sq && !is_cross_cutout(dx as f32, dy as f32, radius) {
                blend_pixel(buffer, width, px, py, color, alpha);
            }
        }
    }
}

fn is_cross_cutout(dx: f32, dy: f32, radius: i32) -> bool {
    let radius = radius as f32;
    let edge_margin = (radius * 0.30).max(2.0);
    let arm_len = (radius - edge_margin).max(1.0);
    let half_thickness = (radius * 0.10).max(1.0);
    distance_to_segment_sq(dx, dy, -arm_len, 0.0, arm_len, 0.0) <= half_thickness * half_thickness
        || distance_to_segment_sq(dx, dy, 0.0, -arm_len, 0.0, arm_len)
            <= half_thickness * half_thickness
}

fn distance_to_segment_sq(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let vx = bx - ax;
    let vy = by - ay;
    let wx = px - ax;
    let wy = py - ay;
    let segment_len_sq = vx * vx + vy * vy;
    if segment_len_sq <= f32::EPSILON {
        let dx = px - ax;
        let dy = py - ay;
        return dx * dx + dy * dy;
    }

    let t = ((wx * vx + wy * vy) / segment_len_sq).clamp(0.0, 1.0);
    let closest_x = ax + t * vx;
    let closest_y = ay + t * vy;
    let dx = px - closest_x;
    let dy = py - closest_y;
    dx * dx + dy * dy
}

fn blend_pixel(buffer: &mut [u8], width: i32, x: i32, y: i32, color: ClickColor, alpha: u8) {
    let index = ((y * width + x) as usize) * 4;
    let inv_alpha = 255u16.saturating_sub(alpha as u16);
    let src_b = premultiply(color.blue, alpha);
    let src_g = premultiply(color.green, alpha);
    let src_r = premultiply(color.red, alpha);

    buffer[index] = src_b.saturating_add(((buffer[index] as u16 * inv_alpha) / 255) as u8);
    buffer[index + 1] = src_g.saturating_add(((buffer[index + 1] as u16 * inv_alpha) / 255) as u8);
    buffer[index + 2] = src_r.saturating_add(((buffer[index + 2] as u16 * inv_alpha) / 255) as u8);
    buffer[index + 3] = alpha.saturating_add(((buffer[index + 3] as u16 * inv_alpha) / 255) as u8);
}

fn premultiply(channel: u8, alpha: u8) -> u8 {
    ((channel as u16 * alpha as u16) / 255) as u8
}

unsafe fn update_layered_window(
    hwnd: HWND,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
    buffer: &[u8],
) {
    let screen_dc = windows_sys::Win32::Graphics::Gdi::GetDC(null_mut());
    if screen_dc.is_null() {
        return;
    }

    let mem_dc = CreateCompatibleDC(screen_dc);
    if mem_dc.is_null() {
        windows_sys::Win32::Graphics::Gdi::ReleaseDC(null_mut(), screen_dc);
        return;
    }

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        },
        ..std::mem::zeroed()
    };
    let mut bits: *mut c_void = null_mut();
    let bitmap = CreateDIBSection(screen_dc, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if bitmap.is_null() || bits.is_null() {
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        DeleteDC(mem_dc);
        windows_sys::Win32::Graphics::Gdi::ReleaseDC(null_mut(), screen_dc);
        return;
    }

    std::ptr::copy_nonoverlapping(buffer.as_ptr(), bits as *mut u8, buffer.len());
    let old_bitmap = SelectObject(mem_dc, bitmap);
    let dst_point = POINT { x: left, y: top };
    let size = SIZE {
        cx: width,
        cy: height,
    };
    let src_point = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };

    UpdateLayeredWindow(
        hwnd, screen_dc, &dst_point, &size, mem_dc, &src_point, 0, &blend, ULW_ALPHA,
    );

    SelectObject(mem_dc, old_bitmap);
    DeleteObject(bitmap);
    DeleteDC(mem_dc);
    windows_sys::Win32::Graphics::Gdi::ReleaseDC(null_mut(), screen_dc);
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
