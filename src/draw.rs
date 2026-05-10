use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    BLENDFUNCTION, CAPTUREBLT, DIB_RGB_COLORS, SRCCOPY,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, ReleaseCapture, SetCapture, SetFocus,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetSystemMetrics, LoadCursorW, RegisterClassExW,
    ShowWindow, UpdateLayeredWindow, CS_HREDRAW, CS_VREDRAW, IDC_ARROW, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOW, ULW_ALPHA, WM_DESTROY,
    WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WNDCLASSEXW,
    WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use crate::win;

const CLASS_NAME: &str = "GMTLDrawOverlay";
const DEFAULT_PEN_WIDTH: i32 = 5;
const MIN_PEN_WIDTH: i32 = 2;
const MAX_PEN_WIDTH: i32 = 40;
const MAX_UNDO: usize = 32;
const VK_CONTROL_KEY: i32 = 0x11;
const VK_SHIFT_KEY: i32 = 0x10;
const VK_TAB_KEY: i32 = 0x09;

static DRAW_HWND: AtomicIsize = AtomicIsize::new(0);
static STATE: Mutex<DrawState> = Mutex::new(DrawState::new());

#[derive(Clone)]
enum DrawCommand {
    Free {
        points: Vec<POINT>,
        style: DrawStyle,
    },
    Line {
        start: POINT,
        end: POINT,
        style: DrawStyle,
    },
    Rect {
        start: POINT,
        end: POINT,
        style: DrawStyle,
    },
    Ellipse {
        start: POINT,
        end: POINT,
        style: DrawStyle,
    },
    Arrow {
        start: POINT,
        end: POINT,
        style: DrawStyle,
    },
    Blur {
        points: Vec<POINT>,
        width: i32,
    },
}

#[derive(Clone, Copy)]
struct DrawStyle {
    color: Rgba,
    width: i32,
}

#[derive(Clone, Copy)]
struct Rgba {
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DrawTool {
    Free,
    Line,
    Rect,
    Ellipse,
    Arrow,
    Blur,
}

struct DraftCommand {
    tool: DrawTool,
    start: POINT,
    points: Vec<POINT>,
    style: DrawStyle,
}

struct DrawState {
    origin_x: i32,
    origin_y: i32,
    width: i32,
    height: i32,
    background: Vec<u8>,
    commands: Vec<DrawCommand>,
    undo: Vec<Vec<DrawCommand>>,
    draft: Option<DraftCommand>,
    current_color: Rgba,
    pen_width: i32,
}

impl DrawState {
    const fn new() -> Self {
        Self {
            origin_x: 0,
            origin_y: 0,
            width: 0,
            height: 0,
            background: Vec::new(),
            commands: Vec::new(),
            undo: Vec::new(),
            draft: None,
            current_color: Rgba {
                red: 255,
                green: 64,
                blue: 64,
                alpha: 255,
            },
            pen_width: DEFAULT_PEN_WIDTH,
        }
    }
}

pub fn show() {
    let existing = DRAW_HWND.load(Ordering::Relaxed);
    if existing != 0 {
        unsafe {
            DestroyWindow(existing as HWND);
        }
        return;
    }

    let left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    if width <= 0 || height <= 0 {
        return;
    }

    let Some(background) = (unsafe { capture_screen(left, top, width, height) }) else {
        return;
    };

    {
        let mut state = STATE.lock().expect("draw mutex poisoned");
        state.origin_x = left;
        state.origin_y = top;
        state.width = width;
        state.height = height;
        state.background = background;
        state.commands.clear();
        state.undo.clear();
        state.draft = None;
        state.current_color = Rgba {
            red: 255,
            green: 64,
            blue: 64,
            alpha: 255,
        };
        state.pen_width = DEFAULT_PEN_WIDTH;
    }

    let instance = unsafe { GetModuleHandleW(null()) };
    let class_name = win::to_wide(CLASS_NAME);
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(draw_proc),
        hInstance: instance,
        hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
        lpszClassName: class_name.as_ptr(),
        ..unsafe { zeroed() }
    };
    unsafe {
        RegisterClassExW(&class);
    }

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            class_name.as_ptr(),
            class_name.as_ptr(),
            WS_POPUP,
            left,
            top,
            width,
            height,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        )
    };
    if hwnd.is_null() {
        return;
    }

    DRAW_HWND.store(hwnd as isize, Ordering::Relaxed);
    unsafe {
        render(hwnd);
        ShowWindow(hwnd, SW_SHOW);
        SetFocus(hwnd);
    }
}

unsafe extern "system" fn draw_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_LBUTTONDOWN => {
            let point = point_from_lparam(lparam);
            {
                let mut state = STATE.lock().expect("draw mutex poisoned");
                let tool = active_tool();
                let style = DrawStyle {
                    color: state.current_color,
                    width: state.pen_width,
                };
                state.draft = Some(DraftCommand {
                    tool,
                    start: point,
                    points: vec![point],
                    style,
                });
            }
            SetCapture(hwnd);
            render(hwnd);
            0
        }
        WM_MOUSEMOVE => {
            let should_render = {
                let mut state = STATE.lock().expect("draw mutex poisoned");
                if let Some(draft) = state.draft.as_mut() {
                    let point = point_from_lparam(lparam);
                    if draft.tool == DrawTool::Free || draft.tool == DrawTool::Blur {
                        draft.points.push(point);
                    } else if draft.points.len() == 1 {
                        draft.points.push(point);
                    } else {
                        draft.points[1] = point;
                    }
                    true
                } else {
                    false
                }
            };
            if should_render {
                render(hwnd);
            }
            0
        }
        WM_LBUTTONUP => {
            ReleaseCapture();
            let should_render = {
                let mut state = STATE.lock().expect("draw mutex poisoned");
                if let Some(draft) = state.draft.take() {
                    if let Some(command) = draft.into_command() {
                        push_undo(&mut state);
                        state.commands.push(command);
                    }
                    true
                } else {
                    false
                }
            };
            if should_render {
                render(hwnd);
            }
            0
        }
        WM_KEYDOWN => {
            if wparam as i32 == 0x1b {
                DestroyWindow(hwnd);
                return 0;
            }
            if handle_key(hwnd, wparam as u32) {
                return 0;
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_MOUSEWHEEL => {
            if is_key_down(VK_CONTROL_KEY) {
                let delta = ((wparam >> 16) & 0xffff) as i16;
                let mut state = STATE.lock().expect("draw mutex poisoned");
                let step = if delta > 0 { 1 } else { -1 };
                state.pen_width = (state.pen_width + step).clamp(MIN_PEN_WIDTH, MAX_PEN_WIDTH);
                return 0;
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_DESTROY => {
            ReleaseCapture();
            DRAW_HWND.store(0, Ordering::Relaxed);
            let mut state = STATE.lock().expect("draw mutex poisoned");
            state.background.clear();
            state.commands.clear();
            state.undo.clear();
            state.draft = None;
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

impl DraftCommand {
    fn into_command(self) -> Option<DrawCommand> {
        let end = *self.points.last()?;
        if self.tool != DrawTool::Free && self.tool != DrawTool::Blur && same_point(self.start, end) {
            return None;
        }

        match self.tool {
            DrawTool::Free if self.points.len() > 1 => Some(DrawCommand::Free {
                points: self.points,
                style: self.style,
            }),
            DrawTool::Line => Some(DrawCommand::Line {
                start: self.start,
                end,
                style: self.style,
            }),
            DrawTool::Rect => Some(DrawCommand::Rect {
                start: self.start,
                end,
                style: self.style,
            }),
            DrawTool::Ellipse => Some(DrawCommand::Ellipse {
                start: self.start,
                end,
                style: self.style,
            }),
            DrawTool::Arrow => Some(DrawCommand::Arrow {
                start: self.start,
                end,
                style: self.style,
            }),
            DrawTool::Blur if self.points.len() > 1 => Some(DrawCommand::Blur {
                points: self.points,
                width: self.style.width.max(12),
            }),
            _ => None,
        }
    }
}

pub fn is_active() -> bool {
    DRAW_HWND.load(Ordering::Relaxed) != 0
}

unsafe fn render(hwnd: HWND) {
    let (left, top, width, height, mut buffer, commands, draft) = {
        let state = STATE.lock().expect("draw mutex poisoned");
        if state.width <= 0 || state.height <= 0 || state.background.is_empty() {
            return;
        }
        (
            state.origin_x,
            state.origin_y,
            state.width,
            state.height,
            state.background.clone(),
            state.commands.clone(),
            state.draft.as_ref().map(draft_to_command),
        )
    };

    for command in &commands {
        draw_command(&mut buffer, width, height, command);
    }
    if let Some(command) = draft {
        draw_command(&mut buffer, width, height, &command);
    }

    update_layered_window(hwnd, left, top, width, height, &buffer);
}

fn draft_to_command(draft: &DraftCommand) -> DrawCommand {
    let end = *draft.points.last().unwrap_or(&draft.start);
    match draft.tool {
        DrawTool::Free => DrawCommand::Free {
            points: draft.points.clone(),
            style: draft.style,
        },
        DrawTool::Line => DrawCommand::Line {
            start: draft.start,
            end,
            style: draft.style,
        },
        DrawTool::Rect => DrawCommand::Rect {
            start: draft.start,
            end,
            style: draft.style,
        },
        DrawTool::Ellipse => DrawCommand::Ellipse {
            start: draft.start,
            end,
            style: draft.style,
        },
        DrawTool::Arrow => DrawCommand::Arrow {
            start: draft.start,
            end,
            style: draft.style,
        },
        DrawTool::Blur => DrawCommand::Blur {
            points: draft.points.clone(),
            width: draft.style.width.max(12),
        },
    }
}

fn draw_command(buffer: &mut [u8], width: i32, height: i32, command: &DrawCommand) {
    match command {
        DrawCommand::Free { points, style } => draw_polyline(buffer, width, height, points, *style),
        DrawCommand::Line { start, end, style } => {
            draw_segment(buffer, width, height, *start, *end, *style)
        }
        DrawCommand::Rect { start, end, style } => {
            draw_rect(buffer, width, height, *start, *end, *style)
        }
        DrawCommand::Ellipse { start, end, style } => {
            draw_ellipse(buffer, width, height, *start, *end, *style)
        }
        DrawCommand::Arrow { start, end, style } => {
            draw_arrow(buffer, width, height, *start, *end, *style)
        }
        DrawCommand::Blur { points, width: blur_width } => {
            draw_blur_stroke(buffer, width, height, points, *blur_width)
        }
    }
}

fn draw_polyline(buffer: &mut [u8], width: i32, height: i32, points: &[POINT], style: DrawStyle) {
    for pair in points.windows(2) {
        draw_segment(buffer, width, height, pair[0], pair[1], style);
    }
}

fn draw_segment(
    buffer: &mut [u8],
    width: i32,
    height: i32,
    start: POINT,
    end: POINT,
    style: DrawStyle,
) {
    let half = (style.width.max(1) as f32) / 2.0;
    let left = (start.x.min(end.x) - style.width).max(0);
    let top = (start.y.min(end.y) - style.width).max(0);
    let right = (start.x.max(end.x) + style.width).min(width - 1);
    let bottom = (start.y.max(end.y) + style.width).min(height - 1);
    let threshold = half * half;

    for y in top..=bottom {
        for x in left..=right {
            let distance = distance_to_segment_sq(
                x as f32,
                y as f32,
                start.x as f32,
                start.y as f32,
                end.x as f32,
                end.y as f32,
            );
            if distance <= threshold {
                blend_pixel(buffer, width, x, y, style.color);
            }
        }
    }
}

fn draw_rect(
    buffer: &mut [u8],
    width: i32,
    height: i32,
    start: POINT,
    end: POINT,
    style: DrawStyle,
) {
    let top_left = POINT {
        x: start.x.min(end.x),
        y: start.y.min(end.y),
    };
    let top_right = POINT {
        x: start.x.max(end.x),
        y: start.y.min(end.y),
    };
    let bottom_left = POINT {
        x: start.x.min(end.x),
        y: start.y.max(end.y),
    };
    let bottom_right = POINT {
        x: start.x.max(end.x),
        y: start.y.max(end.y),
    };

    draw_segment(buffer, width, height, top_left, top_right, style);
    draw_segment(buffer, width, height, top_right, bottom_right, style);
    draw_segment(buffer, width, height, bottom_right, bottom_left, style);
    draw_segment(buffer, width, height, bottom_left, top_left, style);
}

fn draw_ellipse(
    buffer: &mut [u8],
    width: i32,
    height: i32,
    start: POINT,
    end: POINT,
    style: DrawStyle,
) {
    let left = start.x.min(end.x);
    let right = start.x.max(end.x);
    let top = start.y.min(end.y);
    let bottom = start.y.max(end.y);
    let rx = ((right - left) as f32 / 2.0).max(1.0);
    let ry = ((bottom - top) as f32 / 2.0).max(1.0);
    let cx = left as f32 + rx;
    let cy = top as f32 + ry;
    let thickness = style.width.max(1) as f32 / rx.min(ry).max(1.0);

    for y in top.max(0)..=bottom.min(height - 1) {
        for x in left.max(0)..=right.min(width - 1) {
            let nx = (x as f32 - cx) / rx;
            let ny = (y as f32 - cy) / ry;
            let value = nx * nx + ny * ny;
            if (value - 1.0).abs() <= thickness {
                blend_pixel(buffer, width, x, y, style.color);
            }
        }
    }
}

fn draw_arrow(
    buffer: &mut [u8],
    width: i32,
    height: i32,
    start: POINT,
    end: POINT,
    style: DrawStyle,
) {
    draw_segment(buffer, width, height, start, end, style);

    let dx = (end.x - start.x) as f32;
    let dy = (end.y - start.y) as f32;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return;
    }

    let ux = dx / len;
    let uy = dy / len;
    let head_len = (style.width as f32 * 5.0).max(14.0);
    let head_w = (style.width as f32 * 2.5).max(7.0);
    let base_x = end.x as f32 - ux * head_len;
    let base_y = end.y as f32 - uy * head_len;
    let px = -uy;
    let py = ux;
    let left = POINT {
        x: (base_x + px * head_w).round() as i32,
        y: (base_y + py * head_w).round() as i32,
    };
    let right = POINT {
        x: (base_x - px * head_w).round() as i32,
        y: (base_y - py * head_w).round() as i32,
    };

    draw_segment(buffer, width, height, end, left, style);
    draw_segment(buffer, width, height, end, right, style);
}

fn draw_blur_stroke(buffer: &mut [u8], width: i32, height: i32, points: &[POINT], blur_width: i32) {
    if points.len() < 2 {
        return;
    }

    let source = buffer.to_vec();
    for pair in points.windows(2) {
        blur_segment(buffer, &source, width, height, pair[0], pair[1], blur_width);
    }
}

fn blur_segment(
    buffer: &mut [u8],
    source: &[u8],
    width: i32,
    height: i32,
    start: POINT,
    end: POINT,
    blur_width: i32,
) {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let steps = dx.abs().max(dy.abs()).max(1);
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let x = (start.x as f32 + dx as f32 * t).round() as i32;
        let y = (start.y as f32 + dy as f32 * t).round() as i32;
        blur_circle(buffer, source, width, height, x, y, blur_width / 2);
    }
}

fn blur_circle(
    buffer: &mut [u8],
    source: &[u8],
    width: i32,
    height: i32,
    cx: i32,
    cy: i32,
    radius: i32,
) {
    let radius = radius.max(4);
    let left = (cx - radius).max(0);
    let top = (cy - radius).max(0);
    let right = (cx + radius).min(width - 1);
    let bottom = (cy + radius).min(height - 1);
    let radius_sq = radius * radius;

    for y in top..=bottom {
        for x in left..=right {
            let dx = x - cx;
            let dy = y - cy;
            if dx * dx + dy * dy <= radius_sq {
                let (blue, green, red) = average_neighborhood(source, width, height, x, y, 4);
                let index = pixel_index(width, x, y);
                buffer[index] = blue;
                buffer[index + 1] = green;
                buffer[index + 2] = red;
                buffer[index + 3] = 255;
            }
        }
    }
}

fn average_neighborhood(
    source: &[u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    radius: i32,
) -> (u8, u8, u8) {
    let mut blue = 0u32;
    let mut green = 0u32;
    let mut red = 0u32;
    let mut count = 0u32;

    for sy in (y - radius).max(0)..=(y + radius).min(height - 1) {
        for sx in (x - radius).max(0)..=(x + radius).min(width - 1) {
            let index = pixel_index(width, sx, sy);
            blue += source[index] as u32;
            green += source[index + 1] as u32;
            red += source[index + 2] as u32;
            count += 1;
        }
    }

    ((blue / count) as u8, (green / count) as u8, (red / count) as u8)
}

fn handle_key(hwnd: HWND, key: u32) -> bool {
    if is_key_down(VK_CONTROL_KEY) && key == b'Z' as u32 {
        let should_render = {
            let mut state = STATE.lock().expect("draw mutex poisoned");
            if let Some(previous) = state.undo.pop() {
                state.commands = previous;
                state.draft = None;
                true
            } else {
                false
            }
        };
        if should_render {
            unsafe {
                render(hwnd);
            }
        }
        return true;
    }

    let Some(color) = color_from_key(key, is_key_down(VK_SHIFT_KEY)) else {
        return false;
    };

    let mut state = STATE.lock().expect("draw mutex poisoned");
    state.current_color = color;
    true
}

fn color_from_key(key: u32, highlight: bool) -> Option<Rgba> {
    if key == b'X' as u32 {
        return Some(Rgba {
            red: 112,
            green: 112,
            blue: 112,
            alpha: 255,
        });
    }

    let (red, green, blue) = match key {
        value if value == b'R' as u32 => (255, 64, 64),
        value if value == b'G' as u32 => (64, 220, 96),
        value if value == b'B' as u32 => (64, 128, 255),
        value if value == b'Y' as u32 => (255, 224, 64),
        value if value == b'O' as u32 => (255, 144, 32),
        value if value == b'P' as u32 => (192, 96, 255),
        value if value == b'W' as u32 => (255, 255, 255),
        value if value == b'K' as u32 => (0, 0, 0),
        _ => return None,
    };

    Some(Rgba {
        red,
        green,
        blue,
        alpha: if highlight { 96 } else { 255 },
    })
}

fn active_tool() -> DrawTool {
    if is_key_down(b'X' as i32) {
        return DrawTool::Blur;
    }

    let ctrl = is_key_down(VK_CONTROL_KEY);
    let shift = is_key_down(VK_SHIFT_KEY);
    if ctrl && shift {
        DrawTool::Arrow
    } else if ctrl {
        DrawTool::Line
    } else if shift {
        DrawTool::Rect
    } else if is_key_down(VK_TAB_KEY) {
        DrawTool::Ellipse
    } else {
        DrawTool::Free
    }
}

fn push_undo(state: &mut DrawState) {
    if state.undo.len() == MAX_UNDO {
        state.undo.remove(0);
    }
    state.undo.push(state.commands.clone());
}

fn distance_to_segment_sq(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let vx = bx - ax;
    let vy = by - ay;
    let wx = px - ax;
    let wy = py - ay;
    let len_sq = vx * vx + vy * vy;
    if len_sq <= f32::EPSILON {
        let dx = px - ax;
        let dy = py - ay;
        return dx * dx + dy * dy;
    }

    let t = ((wx * vx + wy * vy) / len_sq).clamp(0.0, 1.0);
    let cx = ax + t * vx;
    let cy = ay + t * vy;
    let dx = px - cx;
    let dy = py - cy;
    dx * dx + dy * dy
}

fn blend_pixel(buffer: &mut [u8], width: i32, x: i32, y: i32, color: Rgba) {
    let index = pixel_index(width, x, y);
    let alpha = color.alpha as u32;
    let inv_alpha = 255 - alpha;
    buffer[index] = blend_channel(color.blue, buffer[index], alpha, inv_alpha);
    buffer[index + 1] = blend_channel(color.green, buffer[index + 1], alpha, inv_alpha);
    buffer[index + 2] = blend_channel(color.red, buffer[index + 2], alpha, inv_alpha);
    buffer[index + 3] = 255;
}

fn blend_channel(source: u8, destination: u8, alpha: u32, inv_alpha: u32) -> u8 {
    ((source as u32 * alpha + destination as u32 * inv_alpha) / 255) as u8
}

fn pixel_index(width: i32, x: i32, y: i32) -> usize {
    ((y * width + x) as usize) * 4
}

fn same_point(a: POINT, b: POINT) -> bool {
    a.x == b.x && a.y == b.y
}

fn is_key_down(vk: i32) -> bool {
    unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 }
}

unsafe fn capture_screen(left: i32, top: i32, width: i32, height: i32) -> Option<Vec<u8>> {
    let screen_dc = GetDC(null_mut());
    if screen_dc.is_null() {
        return None;
    }

    let mem_dc = CreateCompatibleDC(screen_dc);
    if mem_dc.is_null() {
        ReleaseDC(null_mut(), screen_dc);
        return None;
    }

    let (bitmap, bits) = create_dib(screen_dc, width, height);
    if bitmap.is_null() || bits.is_null() {
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        DeleteDC(mem_dc);
        ReleaseDC(null_mut(), screen_dc);
        return None;
    }

    let old_bitmap = SelectObject(mem_dc, bitmap);
    let success = BitBlt(
        mem_dc,
        0,
        0,
        width,
        height,
        screen_dc,
        left,
        top,
        SRCCOPY | CAPTUREBLT,
    ) != 0;

    let mut buffer = if success {
        let len = width as usize * height as usize * 4;
        let mut buffer = vec![0u8; len];
        std::ptr::copy_nonoverlapping(bits as *const u8, buffer.as_mut_ptr(), len);
        for pixel in buffer.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
        Some(buffer)
    } else {
        None
    };

    SelectObject(mem_dc, old_bitmap);
    DeleteObject(bitmap);
    DeleteDC(mem_dc);
    ReleaseDC(null_mut(), screen_dc);
    buffer.take()
}

unsafe fn update_layered_window(
    hwnd: HWND,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
    buffer: &[u8],
) {
    let screen_dc = GetDC(null_mut());
    if screen_dc.is_null() {
        return;
    }

    let mem_dc = CreateCompatibleDC(screen_dc);
    if mem_dc.is_null() {
        ReleaseDC(null_mut(), screen_dc);
        return;
    }

    let (bitmap, bits) = create_dib(screen_dc, width, height);
    if bitmap.is_null() || bits.is_null() {
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        DeleteDC(mem_dc);
        ReleaseDC(null_mut(), screen_dc);
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
    ReleaseDC(null_mut(), screen_dc);
}

unsafe fn create_dib(
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    width: i32,
    height: i32,
) -> (windows_sys::Win32::Graphics::Gdi::HBITMAP, *mut c_void) {
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..zeroed()
        },
        ..zeroed()
    };
    let mut bits: *mut c_void = null_mut();
    let bitmap = CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    (bitmap, bits)
}

fn point_from_lparam(lparam: LPARAM) -> POINT {
    POINT {
        x: (lparam as u32 & 0xffff) as i16 as i32,
        y: ((lparam as u32 >> 16) & 0xffff) as i16 as i32,
    }
}
