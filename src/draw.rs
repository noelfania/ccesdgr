use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU8, Ordering};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateBitmap, CreateCompatibleBitmap, CreateCompatibleDC, CreateDIBSection,
    CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, FillRect, GetDC, ReleaseDC, SelectObject,
    SetBkMode, SetDIBits, SetTextColor, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, BLENDFUNCTION, CAPTUREBLT, DIB_RGB_COLORS, DT_CALCRECT, DT_LEFT, DT_NOPREFIX, HDC,
    SRCCOPY,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, ReleaseCapture, SetCapture, SetFocus,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, CreateWindowExW, DefWindowProcW, DestroyCursor, DestroyWindow,
    GetCursorPos, GetSystemMetrics, KillTimer, LoadCursorW, MessageBoxW, RegisterClassExW,
    SetCursor, SetTimer, ShowWindow, WM_CAPTURECHANGED, WM_TIMER,
    UpdateLayeredWindow, CS_HREDRAW, CS_VREDRAW, HCURSOR, HTCLIENT, ICONINFO, IDC_ARROW, IDYES,
    MB_ICONQUESTION, MB_TOPMOST, MB_YESNO,
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOW,
    ULW_ALPHA, WM_DESTROY, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WNDCLASSEXW, WS_EX_LAYERED,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

/// build.rs 가 폴백 CUR도 생성하지만 런타임은 RGBA bin 동적 커서 사용
const CURSOR_SIZE: i32 = 64;
/// 커서 캔버스 안 펜촉(핫스팟) — 두께 원(최대 radius≈20) 여유
const CURSOR_HOT_X: i32 = 22;
const CURSOR_HOT_Y: i32 = 48;

const BLUR_RADIUS: i32 = 6;

use crate::win;

const CLASS_NAME: &str = "GMTLDrawOverlay";
const DEFAULT_PEN_WIDTH: i32 = 5;
const MIN_PEN_WIDTH: i32 = 2;
const MAX_PEN_WIDTH: i32 = 40;
const MAX_UNDO: usize = 32;
const VK_CONTROL_KEY: i32 = 0x11;
const VK_SHIFT_KEY: i32 = 0x10;
const VK_TAB_KEY: i32 = 0x09;
const VK_F1_KEY: u32 = 0x70;
const TIMER_HINT: usize = 1;
const HINT_DURATION_MS: u32 = 3500;

// 도움말 표시 상태: 진입 힌트 → 자동 숨김, F1로 전체 단축키 토글
const HELP_NONE: u8 = 0;
const HELP_HINT: u8 = 1;
const HELP_FULL: u8 = 2;
static HELP_MODE: AtomicU8 = AtomicU8::new(HELP_NONE);
/// 닫기 확인 창이 떠 있는 동안 단축키·Esc로 또 띄우지 않는다.
static CONFIRMING_CLOSE: AtomicBool = AtomicBool::new(false);

const HINT_TEXT: &str =
    "Annotate mode  ·  F1: shortcuts  ·  Ctrl+C: copy  ·  Ctrl+S: save PNG  ·  Esc: exit";
const HELP_TITLE: &str = "Annotation shortcuts";
const HELP_ROWS: &[(&str, &str)] = &[
    ("Drag", "Free pen"),
    ("Ctrl + drag", "Line"),
    ("Shift + drag", "Rectangle"),
    ("Tab + drag", "Ellipse"),
    ("Ctrl+Shift + drag", "Arrow"),
    ("X + drag", "Blur area"),
    ("Right-drag", "Eraser"),
    ("Ctrl + Wheel", "Pen width"),
    ("R G B Y O P W K", "Color (Shift = highlighter)"),
    ("Ctrl+Z", "Undo"),
    ("Ctrl+C", "Copy annotated screen"),
    ("Ctrl+S", "Save annotated screen as PNG"),
    ("F1", "Show / hide this help"),
    ("Esc", "Exit"),
];

// build.rs: assets/draw-pen.png → RGBA bin(동적 틴트)
static DRAW_PEN_RGBA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/draw_pen_rgba.bin"));

static DRAW_HWND: AtomicIsize = AtomicIsize::new(0);
static STATE: Mutex<DrawState> = Mutex::new(DrawState::new());
static CURSOR_CACHE: Mutex<Option<CachedCursor>> = Mutex::new(None);

struct CachedCursor {
    tool: DrawTool,
    color: Rgba,
    width: i32,
    handle: isize,
}

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
    /// 사각 영역 블러
    Blur {
        start: POINT,
        end: POINT,
    },
    /// 원본 배경 복원 지우개 스트로크
    Erase {
        points: Vec<POINT>,
        width: i32,
    },
}

#[derive(Clone, Copy)]
struct DrawStyle {
    color: Rgba,
    width: i32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
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
    Erase,
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
    /// 원본 화면 캡처 (지우개 복원용)
    background: Vec<u8>,
    /// 확정된 스트로크가 베이크된 버퍼 (매 프레임 전체 리플레이 방지)
    committed: Vec<u8>,
    commands: Vec<DrawCommand>,
    undo: Vec<Vec<DrawCommand>>,
    draft: Option<DraftCommand>,
    cursor_pos: Option<POINT>,
    current_color: Rgba,
    pen_width: i32,
    /// 복사·저장 이후 바뀐 내용이 있으면 true (닫기 확인 여부)
    dirty: bool,
}

impl DrawState {
    const fn new() -> Self {
        Self {
            origin_x: 0,
            origin_y: 0,
            width: 0,
            height: 0,
            background: Vec::new(),
            committed: Vec::new(),
            commands: Vec::new(),
            undo: Vec::new(),
            draft: None,
            cursor_pos: None,
            current_color: Rgba {
                red: 255,
                green: 64,
                blue: 64,
                alpha: 255,
            },
            pen_width: DEFAULT_PEN_WIDTH,
            dirty: false,
        }
    }
}

/// 복사·저장하지 않은 주석이 있으면 확인 후 닫는다.
unsafe fn close_with_confirm(hwnd: HWND) {
    if CONFIRMING_CLOSE.load(Ordering::Relaxed) {
        return;
    }
    let dirty = STATE.lock().expect("draw mutex poisoned").dirty;
    if dirty {
        CONFIRMING_CLOSE.store(true, Ordering::Relaxed);
        let title = win::to_wide("GMTL Mouse Click");
        let message = win::to_wide("Discard your annotations?\n\nUse Ctrl+C to copy or Ctrl+S to save them first.");
        let answer = MessageBoxW(
            hwnd,
            message.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_ICONQUESTION | MB_TOPMOST,
        );
        CONFIRMING_CLOSE.store(false, Ordering::Relaxed);
        if answer != IDYES {
            return;
        }
    }
    DestroyWindow(hwnd);
}

pub fn show() {
    let existing = DRAW_HWND.load(Ordering::Relaxed);
    if existing != 0 {
        unsafe {
            close_with_confirm(existing as HWND);
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
        crate::tray::notify_error("Annotate failed: could not capture the screen.");
        return;
    };

    {
        let mut state = STATE.lock().expect("draw mutex poisoned");
        state.origin_x = left;
        state.origin_y = top;
        state.width = width;
        state.height = height;
        state.background = background.clone();
        state.committed = background;
        state.commands.clear();
        state.undo.clear();
        state.draft = None;
        state.cursor_pos = current_cursor_pos(left, top);
        state.current_color = Rgba {
            red: 255,
            green: 64,
            blue: 64,
            alpha: 255,
        };
        state.pen_width = DEFAULT_PEN_WIDTH;
        state.dirty = false;
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
        crate::tray::notify_error("Annotate failed: could not open the overlay window.");
        return;
    }

    DRAW_HWND.store(hwnd as isize, Ordering::Relaxed);
    HELP_MODE.store(HELP_HINT, Ordering::Relaxed);
    unsafe {
        SetTimer(hwnd, TIMER_HINT, HINT_DURATION_MS, None);
        render(hwnd);
        ShowWindow(hwnd, SW_SHOW);
        SetFocus(hwnd);
        apply_tool_cursor();
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
                if state.draft.is_some() {
                    return 0;
                }
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
                state.cursor_pos = Some(point);
            }
            SetCapture(hwnd);
            apply_tool_cursor();
            render(hwnd);
            0
        }
        WM_RBUTTONDOWN => {
            let point = point_from_lparam(lparam);
            {
                let mut state = STATE.lock().expect("draw mutex poisoned");
                if state.draft.is_some() {
                    return 0;
                }
                let style = DrawStyle {
                    color: state.current_color,
                    width: state.pen_width,
                };
                state.draft = Some(DraftCommand {
                    tool: DrawTool::Erase,
                    start: point,
                    points: vec![point],
                    style,
                });
                state.cursor_pos = Some(point);
            }
            SetCapture(hwnd);
            apply_tool_cursor();
            render(hwnd);
            0
        }
        WM_MOUSEMOVE => {
            let needs_render = {
                let mut state = STATE.lock().expect("draw mutex poisoned");
                let point = point_from_lparam(lparam);
                state.cursor_pos = Some(point);
                if let Some(draft) = state.draft.as_mut() {
                    if draft.tool == DrawTool::Free || draft.tool == DrawTool::Erase {
                        draft.points.push(point);
                    } else if draft.points.len() == 1 {
                        draft.points.push(point);
                    } else {
                        draft.points[1] = point;
                    }
                }
                state.draft.is_some()
            };
            apply_tool_cursor();
            if needs_render {
                render(hwnd);
            }
            0
        }
        WM_LBUTTONUP | WM_RBUTTONUP => {
            let point = point_from_lparam(lparam);
            let should_render = {
                let mut state = STATE.lock().expect("draw mutex poisoned");
                state.cursor_pos = Some(point);
                if let Some(draft) = state.draft.take() {
                    if let Some(command) = draft.into_command() {
                        push_undo(&mut state);
                        bake_command(&mut state, &command);
                        state.commands.push(command);
                        state.dirty = true;
                    }
                    true
                } else {
                    false
                }
            };
            // draft를 확정한 뒤 해제 — WM_CAPTURECHANGED가 진행 중 draft를 버리기 때문
            ReleaseCapture();
            apply_tool_cursor();
            if should_render {
                render(hwnd);
            }
            0
        }
        // 다른 창이 캡처를 가져가면 버튼을 뗀 이벤트가 오지 않으므로 그리던 것을 취소한다.
        WM_CAPTURECHANGED => {
            let had_draft = STATE
                .lock()
                .expect("draw mutex poisoned")
                .draft
                .take()
                .is_some();
            if had_draft {
                render(hwnd);
            }
            0
        }
        WM_KEYDOWN => {
            if wparam as i32 == 0x1b {
                close_with_confirm(hwnd);
                return 0;
            }
            if wparam as u32 == VK_F1_KEY {
                let next = if HELP_MODE.load(Ordering::Relaxed) == HELP_FULL {
                    HELP_NONE
                } else {
                    HELP_FULL
                };
                HELP_MODE.store(next, Ordering::Relaxed);
                KillTimer(hwnd, TIMER_HINT);
                render(hwnd);
                return 0;
            }
            if is_key_down(VK_CONTROL_KEY) && wparam as u32 == b'C' as u32 {
                copy_to_clipboard(hwnd);
                return 0;
            }
            if is_key_down(VK_CONTROL_KEY) && wparam as u32 == b'S' as u32 {
                save_to_png();
                return 0;
            }
            apply_tool_cursor();
            if handle_key(hwnd, wparam as u32) {
                return 0;
            }
            // X(Blur) 등 도구 전환 시 프리뷰 갱신
            if wparam as u32 == b'X' as u32
                || wparam as i32 == VK_CONTROL_KEY
                || wparam as i32 == VK_SHIFT_KEY
                || wparam as i32 == VK_TAB_KEY
            {
                render(hwnd);
                return 0;
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_KEYUP => {
            apply_tool_cursor();
            if active_tool() == DrawTool::Blur || wparam as u32 == b'X' as u32 {
                render(hwnd);
            }
            0
        }
        WM_SETCURSOR => {
            if win::low_word(lparam as usize) as u32 == HTCLIENT {
                apply_tool_cursor();
                return 1;
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_MOUSEWHEEL => {
            if is_key_down(VK_CONTROL_KEY) {
                let delta = ((wparam >> 16) & 0xffff) as i16;
                let mut state = STATE.lock().expect("draw mutex poisoned");
                let step = if delta > 0 { 1 } else { -1 };
                state.pen_width = (state.pen_width + step).clamp(MIN_PEN_WIDTH, MAX_PEN_WIDTH);
                state.cursor_pos = Some(screen_point_from_lparam(lparam, state.origin_x, state.origin_y));
                drop(state);
                apply_tool_cursor();
                render(hwnd);
                return 0;
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_TIMER if wparam == TIMER_HINT => {
            KillTimer(hwnd, TIMER_HINT);
            if HELP_MODE.load(Ordering::Relaxed) == HELP_HINT {
                HELP_MODE.store(HELP_NONE, Ordering::Relaxed);
                render(hwnd);
            }
            0
        }
        WM_DESTROY => {
            ReleaseCapture();
            KillTimer(hwnd, TIMER_HINT);
            HELP_MODE.store(HELP_NONE, Ordering::Relaxed);
            DRAW_HWND.store(0, Ordering::Relaxed);
            clear_cursor_cache();
            let mut state = STATE.lock().expect("draw mutex poisoned");
            state.background.clear();
            state.committed.clear();
            state.commands.clear();
            state.undo.clear();
            state.draft = None;
            state.cursor_pos = None;
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

impl DraftCommand {
    fn into_command(self) -> Option<DrawCommand> {
        let end = *self.points.last()?;
        if self.tool != DrawTool::Free
            && self.tool != DrawTool::Erase
            && same_point(self.start, end)
        {
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
            DrawTool::Blur => Some(DrawCommand::Blur {
                start: self.start,
                end,
            }),
            DrawTool::Erase if self.points.len() > 1 => Some(DrawCommand::Erase {
                points: self.points,
                width: self.style.width.max(MIN_PEN_WIDTH),
            }),
            _ => None,
        }
    }
}

pub fn is_active() -> bool {
    DRAW_HWND.load(Ordering::Relaxed) != 0
}

unsafe fn render(hwnd: HWND) {
    let (left, top, width, height, background, mut buffer, draft) = {
        let state = STATE.lock().expect("draw mutex poisoned");
        if state.width <= 0 || state.height <= 0 || state.committed.is_empty() {
            return;
        }
        (
            state.origin_x,
            state.origin_y,
            state.width,
            state.height,
            state.background.clone(),
            state.committed.clone(),
            state.draft.as_ref().map(|d| DraftCommand {
                tool: d.tool,
                start: d.start,
                points: d.points.clone(),
                style: d.style,
            }),
        )
    };

    // 확정 버퍼 위에 draft만 합성 (블러 등 과거 커맨드 재실행 안 함)
    if let Some(draft) = draft.as_ref() {
        if draft.tool == DrawTool::Blur {
            let end = *draft.points.last().unwrap_or(&draft.start);
            draw_blur_selection_outline(&mut buffer, width, height, draft.start, end);
        } else if let Some(command) = draft_to_command(draft) {
            draw_command(&mut buffer, &background, width, height, &command);
        }
    }

    update_layered_window(hwnd, left, top, width, height, &buffer);
}

/// 확정 커맨드를 committed 버퍼에 한 번만 베이크
fn bake_command(state: &mut DrawState, command: &DrawCommand) {
    if state.committed.is_empty() {
        state.committed = state.background.clone();
    }
    let width = state.width;
    let height = state.height;
    let background = state.background.clone();
    draw_command(&mut state.committed, &background, width, height, command);
}

/// undo 후 background + commands로 committed 재구성
fn recompose_committed(state: &mut DrawState) {
    let width = state.width;
    let height = state.height;
    let background = state.background.clone();
    let mut buffer = background.clone();
    for command in &state.commands {
        draw_command(&mut buffer, &background, width, height, command);
    }
    state.committed = buffer;
}

fn draft_to_command(draft: &DraftCommand) -> Option<DrawCommand> {
    let end = *draft.points.last().unwrap_or(&draft.start);
    match draft.tool {
        DrawTool::Free => Some(DrawCommand::Free {
            points: draft.points.clone(),
            style: draft.style,
        }),
        DrawTool::Line => Some(DrawCommand::Line {
            start: draft.start,
            end,
            style: draft.style,
        }),
        DrawTool::Rect => Some(DrawCommand::Rect {
            start: draft.start,
            end,
            style: draft.style,
        }),
        DrawTool::Ellipse => Some(DrawCommand::Ellipse {
            start: draft.start,
            end,
            style: draft.style,
        }),
        DrawTool::Arrow => Some(DrawCommand::Arrow {
            start: draft.start,
            end,
            style: draft.style,
        }),
        DrawTool::Blur => Some(DrawCommand::Blur {
            start: draft.start,
            end,
        }),
        DrawTool::Erase => Some(DrawCommand::Erase {
            points: draft.points.clone(),
            width: draft.style.width.max(MIN_PEN_WIDTH),
        }),
    }
}

fn draw_command(
    buffer: &mut [u8],
    background: &[u8],
    width: i32,
    height: i32,
    command: &DrawCommand,
) {
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
        DrawCommand::Blur { start, end } => {
            draw_blur_rect(buffer, width, height, *start, *end)
        }
        DrawCommand::Erase { points, width: erase_width } => {
            draw_erase_stroke(buffer, background, width, height, points, *erase_width)
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
    let pen = style.width.max(1);
    let thickness = pen as f32 / rx.min(ry).max(1.0);
    // 선 두께 바깥쪽이 드래그 박스 밖으로 나가므로 스캔 범위를 확장
    let scan_left = (left - pen).max(0);
    let scan_top = (top - pen).max(0);
    let scan_right = (right + pen).min(width - 1);
    let scan_bottom = (bottom + pen).min(height - 1);

    for y in scan_top..=scan_bottom {
        for x in scan_left..=scan_right {
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

fn draw_blur_rect(buffer: &mut [u8], width: i32, height: i32, start: POINT, end: POINT) {
    let left = start.x.min(end.x).max(0);
    let right = start.x.max(end.x).min(width - 1);
    let top = start.y.min(end.y).max(0);
    let bottom = start.y.max(end.y).min(height - 1);
    if left > right || top > bottom {
        return;
    }

    let source = buffer.to_vec();
    // 픽셀마다 이웃을 다시 더하지 않고 열별 합을 밀어 가며 계산 (화면 크기 블러에서 UI 멈춤 방지)
    let first_col = (left - BLUR_RADIUS).max(0);
    let last_col = (right + BLUR_RADIUS).min(width - 1);
    let mut column_sums = vec![[0u32; 3]; (last_col - first_col + 1) as usize];
    let add_row = |sums: &mut [[u32; 3]], row: i32, add: bool| {
        for (sum, col) in sums.iter_mut().zip(first_col..=last_col) {
            let index = pixel_index(width, col, row);
            for (channel, value) in sum.iter_mut().zip(&source[index..index + 3]) {
                if add {
                    *channel += *value as u32;
                } else {
                    *channel -= *value as u32;
                }
            }
        }
    };

    for row in (top - BLUR_RADIUS).max(0)..=(top + BLUR_RADIUS).min(height - 1) {
        add_row(&mut column_sums, row, true);
    }

    for y in top..=bottom {
        if y > top {
            if y - BLUR_RADIUS - 1 >= 0 {
                add_row(&mut column_sums, y - BLUR_RADIUS - 1, false);
            }
            if y + BLUR_RADIUS < height {
                add_row(&mut column_sums, y + BLUR_RADIUS, true);
            }
        }
        let rows = ((y + BLUR_RADIUS).min(height - 1) - (y - BLUR_RADIUS).max(0) + 1) as u32;

        let mut window = [0u32; 3];
        for col in (left - BLUR_RADIUS).max(0)..=(left + BLUR_RADIUS).min(width - 1) {
            let sum = column_sums[(col - first_col) as usize];
            window = [window[0] + sum[0], window[1] + sum[1], window[2] + sum[2]];
        }
        for x in left..=right {
            if x > left {
                if x - BLUR_RADIUS - 1 >= 0 {
                    let sum = column_sums[(x - BLUR_RADIUS - 1 - first_col) as usize];
                    window = [window[0] - sum[0], window[1] - sum[1], window[2] - sum[2]];
                }
                if x + BLUR_RADIUS < width {
                    let sum = column_sums[(x + BLUR_RADIUS - first_col) as usize];
                    window = [window[0] + sum[0], window[1] + sum[1], window[2] + sum[2]];
                }
            }
            let cols = ((x + BLUR_RADIUS).min(width - 1) - (x - BLUR_RADIUS).max(0) + 1) as u32;
            let count = rows * cols;
            let index = pixel_index(width, x, y);
            buffer[index] = (window[0] / count) as u8;
            buffer[index + 1] = (window[1] / count) as u8;
            buffer[index + 2] = (window[2] / count) as u8;
            buffer[index + 3] = 255;
        }
    }
}

fn draw_blur_selection_outline(
    buffer: &mut [u8],
    width: i32,
    height: i32,
    start: POINT,
    end: POINT,
) {
    let style_white = DrawStyle {
        color: Rgba {
            red: 255,
            green: 255,
            blue: 255,
            alpha: 220,
        },
        width: 2,
    };
    let style_black = DrawStyle {
        color: Rgba {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 180,
        },
        width: 1,
    };
    draw_rect(buffer, width, height, start, end, style_black);
    draw_rect(buffer, width, height, start, end, style_white);
}

fn draw_erase_stroke(
    buffer: &mut [u8],
    background: &[u8],
    width: i32,
    height: i32,
    points: &[POINT],
    erase_width: i32,
) {
    if points.is_empty() {
        return;
    }
    if points.len() == 1 {
        erase_segment(
            buffer,
            background,
            width,
            height,
            points[0],
            points[0],
            erase_width,
        );
        return;
    }
    for pair in points.windows(2) {
        erase_segment(
            buffer,
            background,
            width,
            height,
            pair[0],
            pair[1],
            erase_width,
        );
    }
}

fn erase_segment(
    buffer: &mut [u8],
    background: &[u8],
    width: i32,
    height: i32,
    start: POINT,
    end: POINT,
    erase_width: i32,
) {
    let erase_width = erase_width.max(1);
    let half = erase_width as f32 / 2.0;
    let left = (start.x.min(end.x) - erase_width).max(0);
    let top = (start.y.min(end.y) - erase_width).max(0);
    let right = (start.x.max(end.x) + erase_width).min(width - 1);
    let bottom = (start.y.max(end.y) + erase_width).min(height - 1);
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
                let index = pixel_index(width, x, y);
                buffer[index] = background[index];
                buffer[index + 1] = background[index + 1];
                buffer[index + 2] = background[index + 2];
                buffer[index + 3] = background[index + 3];
            }
        }
    }
}

#[cfg(test)]
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
                state.dirty = !state.commands.is_empty();
                recompose_committed(&mut state);
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
    drop(state);
    unsafe {
        apply_tool_cursor();
        render(hwnd);
    }
    true
}

fn color_from_key(key: u32, highlight: bool) -> Option<Rgba> {
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

fn current_cursor_pos(origin_x: i32, origin_y: i32) -> Option<POINT> {
    let mut point = POINT { x: 0, y: 0 };
    let success = unsafe { GetCursorPos(&mut point) } != 0;
    if success {
        Some(POINT {
            x: point.x - origin_x,
            y: point.y - origin_y,
        })
    } else {
        None
    }
}

fn screen_point_from_lparam(lparam: LPARAM, origin_x: i32, origin_y: i32) -> POINT {
    let point = point_from_lparam(lparam);
    POINT {
        x: point.x - origin_x,
        y: point.y - origin_y,
    }
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
    let help_mode = HELP_MODE.load(Ordering::Relaxed);
    if help_mode != HELP_NONE {
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u8, buffer.len());
        let full = help_mode == HELP_FULL;
        draw_help_panel(hwnd, mem_dc, pixels, left, top, width, height, full);
    }
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

/// 주 모니터 상단 중앙에 어두운 패널 + 흰 글씨 (진입 힌트 / F1 단축키 표)
unsafe fn draw_help_panel(
    hwnd: HWND,
    hdc: HDC,
    pixels: &mut [u8],
    left: i32,
    top: i32,
    width: i32,
    height: i32,
    full: bool,
) {
    let dpi = win::dpi_for_window(hwnd);
    let font = win::create_ui_font(dpi * 4 / 3); // 9pt × 4/3 ≈ 12pt
    let old_font = SelectObject(hdc, font);

    let measure = |text: &str| -> (i32, i32) {
        let wide = win::to_wide(text);
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        DrawTextW(
            hdc,
            wide.as_ptr(),
            (wide.len() - 1) as i32,
            &mut rect,
            DT_LEFT | DT_NOPREFIX | DT_CALCRECT,
        );
        (rect.right, rect.bottom)
    };

    let pad = win::scale(14, dpi);
    let gap = win::scale(24, dpi);
    let (line_w, line_h) = measure(HINT_TEXT);
    let (content_w, content_h, key_col_w) = if full {
        let key_col_w = HELP_ROWS.iter().map(|(key, _)| measure(key).0).max().unwrap_or(0);
        let desc_col_w = HELP_ROWS.iter().map(|(_, desc)| measure(desc).0).max().unwrap_or(0);
        let title_w = measure(HELP_TITLE).0;
        (
            (key_col_w + gap + desc_col_w).max(title_w),
            line_h * (HELP_ROWS.len() as i32 + 2),
            key_col_w,
        )
    } else {
        (line_w, line_h, 0)
    };

    let primary = win::primary_screen_rect();
    let panel_w = content_w + pad * 2;
    let panel_h = content_h + pad * 2;
    // 버퍼 좌표 = 화면 좌표 - 가상 화면 원점
    let panel_left = ((primary.left + primary.right) / 2 - panel_w / 2 - left).max(0);
    let panel_top = (primary.top + win::scale(24, dpi) - top).max(0);
    let panel = RECT {
        left: panel_left,
        top: panel_top,
        right: (panel_left + panel_w).min(width),
        bottom: (panel_top + panel_h).min(height),
    };

    let brush = CreateSolidBrush(win::colorref(32, 32, 32));
    FillRect(hdc, &panel, brush);
    DeleteObject(brush);
    SetBkMode(hdc, 1);

    let draw_line = |text: &str, x: i32, y: i32, color: (u8, u8, u8)| {
        SetTextColor(hdc, win::colorref(color.0, color.1, color.2));
        let wide = win::to_wide(text);
        let mut rect = RECT {
            left: x,
            top: y,
            right: panel.right,
            bottom: y + line_h,
        };
        DrawTextW(
            hdc,
            wide.as_ptr(),
            (wide.len() - 1) as i32,
            &mut rect,
            DT_LEFT | DT_NOPREFIX,
        );
    };

    let x = panel.left + pad;
    let y = panel.top + pad;
    if full {
        draw_line(HELP_TITLE, x, y, (255, 255, 255));
        for (index, (key, desc)) in HELP_ROWS.iter().enumerate() {
            let row_y = y + line_h * (index as i32 + 2);
            draw_line(key, x, row_y, (255, 214, 102));
            draw_line(desc, x + key_col_w + gap, row_y, (235, 235, 235));
        }
    } else {
        draw_line(HINT_TEXT, x, y, (255, 255, 255));
    }

    SelectObject(hdc, old_font);
    DeleteObject(font);

    // GDI는 32bpp DIB의 alpha를 0으로 남기므로 패널 영역을 불투명으로 복구
    for py in panel.top..panel.bottom {
        for px in panel.left..panel.right {
            pixels[pixel_index(width, px, py) + 3] = 255;
        }
    }
}

/// 현재 주석 결과(확정 버퍼)를 클립보드에 비트맵으로 복사
unsafe fn copy_to_clipboard(hwnd: HWND) {
    let (width, height, pixels) = {
        let state = STATE.lock().expect("draw mutex poisoned");
        (state.width, state.height, state.committed.clone())
    };
    if width <= 0 || height <= 0 || pixels.is_empty() {
        return;
    }

    let screen_dc = GetDC(null_mut());
    if screen_dc.is_null() {
        return;
    }
    let bitmap = CreateCompatibleBitmap(screen_dc, width, height);
    if bitmap.is_null() {
        ReleaseDC(null_mut(), screen_dc);
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
            ..zeroed()
        },
        ..zeroed()
    };
    let rows = SetDIBits(
        screen_dc,
        bitmap,
        0,
        height as u32,
        pixels.as_ptr() as *const c_void,
        &bmi,
        DIB_RGB_COLORS,
    );
    ReleaseDC(null_mut(), screen_dc);
    if rows == 0 {
        DeleteObject(bitmap);
        crate::tray::notify_error("Failed to copy the annotation to the clipboard.");
        return;
    }

    if crate::snip::set_clipboard_bitmap(hwnd, bitmap).is_ok() {
        STATE.lock().expect("draw mutex poisoned").dirty = false;
        crate::tray::notify("Annotated screen copied to clipboard");
    } else {
        crate::tray::notify_error("Failed to copy the annotation to the clipboard.");
    }
}

/// 주석이 합성된 화면을 스냅샷 폴더에 PNG로 저장한다.
fn save_to_png() {
    let Some(config) = crate::current_config() else {
        return;
    };
    let (width, height, pixels) = {
        let state = STATE.lock().expect("draw mutex poisoned");
        (state.width, state.height, state.committed.clone())
    };
    if width <= 0 || height <= 0 || pixels.is_empty() {
        return;
    }
    match crate::snip::save_png_file(
        Path::new(&config.snapshot_output_path),
        &pixels,
        width,
        height,
    ) {
        Ok(path) => {
            STATE.lock().expect("draw mutex poisoned").dirty = false;
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            crate::tray::notify(&format!("Annotated screen saved: {name}"));
        }
        Err(message) => {
            crate::tray::notify_error(&format!("Failed to save the PNG: {message}"));
        }
    }
}

#[cfg(test)]
mod blur_tests {
    use super::*;

    #[test]
    fn sliding_blur_matches_naive_average() {
        let (width, height) = (40, 30);
        let mut source = vec![0u8; (width * height * 4) as usize];
        for (index, byte) in source.iter_mut().enumerate() {
            *byte = (index.wrapping_mul(2654435761) >> 7) as u8;
        }
        // 화면 가장자리에 걸친 영역과 안쪽 영역 모두 확인
        for (start, end) in [
            (POINT { x: 0, y: 0 }, POINT { x: 39, y: 29 }),
            (POINT { x: 10, y: 8 }, POINT { x: 20, y: 15 }),
            (POINT { x: 35, y: 25 }, POINT { x: 39, y: 29 }),
        ] {
            let mut blurred = source.clone();
            draw_blur_rect(&mut blurred, width, height, start, end);
            for y in start.y..=end.y {
                for x in start.x..=end.x {
                    let (blue, green, red) =
                        average_neighborhood(&source, width, height, x, y, BLUR_RADIUS);
                    let index = pixel_index(width, x, y);
                    assert_eq!(
                        (blurred[index], blurred[index + 1], blurred[index + 2]),
                        (blue, green, red),
                        "pixel ({x},{y})"
                    );
                }
            }
        }
    }
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

/// 색·두께·도구에 맞는 동적 펜 커서 적용 (드래프트 중이면 그 도구 우선)
unsafe fn apply_tool_cursor() {
    let (tool, color, width) = {
        let state = STATE.lock().expect("draw mutex poisoned");
        let tool = state
            .draft
            .as_ref()
            .map(|draft| draft.tool)
            .unwrap_or_else(active_tool);
        (tool, state.current_color, state.pen_width)
    };
    SetCursor(dynamic_pen_cursor(tool, color, width));
}

fn clear_cursor_cache() {
    let mut cache = CURSOR_CACHE.lock().expect("cursor cache poisoned");
    if let Some(old) = cache.take() {
        unsafe {
            DestroyCursor(old.handle as HCURSOR);
        }
    }
}

unsafe fn dynamic_pen_cursor(tool: DrawTool, color: Rgba, width: i32) -> HCURSOR {
    {
        let cache = CURSOR_CACHE.lock().expect("cursor cache poisoned");
        if let Some(cached) = cache.as_ref() {
            if cached.tool == tool && cached.color == color && cached.width == width {
                return cached.handle as HCURSOR;
            }
        }
    }

    let Some(handle) = create_dynamic_pen_cursor(tool, color, width) else {
        return LoadCursorW(null_mut(), IDC_ARROW);
    };
    let mut cache = CURSOR_CACHE.lock().expect("cursor cache poisoned");
    if let Some(old) = cache.replace(CachedCursor {
        tool,
        color,
        width,
        handle: handle as isize,
    }) {
        DestroyCursor(old.handle as HCURSOR);
    }
    handle
}

unsafe fn create_dynamic_pen_cursor(tool: DrawTool, color: Rgba, width: i32) -> Option<HCURSOR> {
    let (bgra, hot_x, hot_y) = compose_cursor_bgra(tool, color, width);
    cursor_from_bgra(&bgra, CURSOR_SIZE, hot_x, hot_y)
}

fn compose_cursor_bgra(tool: DrawTool, color: Rgba, pen_width: i32) -> (Vec<u8>, i32, i32) {
    let size = CURSOR_SIZE as usize;
    let mut bgra = vec![0u8; size * size * 4];
    let hot_x = CURSOR_HOT_X;
    let hot_y = CURSOR_HOT_Y;

    blit_tinted_pen(&mut bgra, color, hot_x, hot_y);

    let radius = (pen_width as f32 / 2.0).ceil() as i32;
    let radius = radius.clamp(1, 18);
    draw_thickness_ring(&mut bgra, hot_x, hot_y, radius, color);

    match tool {
        DrawTool::Ellipse => draw_badge_circle(&mut bgra, hot_x - 10, hot_y - 14, 5),
        DrawTool::Rect => draw_badge_rect(&mut bgra, hot_x - 14, hot_y - 18, 10, 10),
        DrawTool::Arrow => draw_badge_up_arrow(&mut bgra, hot_x - 10, hot_y - 20, 9),
        DrawTool::Blur => draw_badge_blur_text(&mut bgra, hot_x - 2, hot_y - 22),
        DrawTool::Line => draw_badge_line(&mut bgra, hot_x - 14, hot_y - 16, 12),
        DrawTool::Erase => draw_badge_eraser(&mut bgra, hot_x - 14, hot_y - 20),
        DrawTool::Free => {}
    }

    (bgra, hot_x, hot_y)
}

fn pen_rgba_meta() -> (u32, u32, u32, u32, &'static [u8]) {
    let w = u32::from_le_bytes(DRAW_PEN_RGBA[0..4].try_into().unwrap());
    let h = u32::from_le_bytes(DRAW_PEN_RGBA[4..8].try_into().unwrap());
    let hx = u32::from_le_bytes(DRAW_PEN_RGBA[8..12].try_into().unwrap());
    let hy = u32::from_le_bytes(DRAW_PEN_RGBA[12..16].try_into().unwrap());
    let pixels = &DRAW_PEN_RGBA[16..];
    (w, h, hx, hy, pixels)
}

fn blit_tinted_pen(bgra: &mut [u8], color: Rgba, hot_x: i32, hot_y: i32) {
    let (pw, ph, src_hx, src_hy, pixels) = pen_rgba_meta();
    let ox = hot_x - src_hx as i32;
    let oy = hot_y - src_hy as i32;
    for sy in 0..ph as i32 {
        for sx in 0..pw as i32 {
            let si = ((sy as u32 * pw + sx as u32) * 4) as usize;
            let sr = pixels[si];
            let sg = pixels[si + 1];
            let sb = pixels[si + 2];
            let sa = pixels[si + 3];
            if sa < 16 {
                continue;
            }
            let (r, g, b) = tint_pen_pixel(sr, sg, sb, color);
            put_bgra(bgra, ox + sx, oy + sy, r, g, b, sa);
        }
    }
}

fn tint_pen_pixel(sr: u8, sg: u8, sb: u8, color: Rgba) -> (u8, u8, u8) {
    // 어두운 회색 클립은 유지, 그 외는 밝기 유지하며 현재 색으로 틴트
    let max = sr.max(sg).max(sb) as i32;
    let min = sr.min(sg).min(sb) as i32;
    if max - min < 25 && max < 100 {
        return (sr, sg, sb);
    }
    let lum = ((sr as u32) * 30 + (sg as u32) * 59 + (sb as u32) * 11) / 100;
    let scale = lum.max(40);
    (
        ((color.red as u32 * scale) / 255).min(255) as u8,
        ((color.green as u32 * scale) / 255).min(255) as u8,
        ((color.blue as u32 * scale) / 255).min(255) as u8,
    )
}

fn draw_thickness_ring(bgra: &mut [u8], cx: i32, cy: i32, radius: i32, color: Rgba) {
    let fill = Rgba {
        alpha: color.alpha.max(80).min(140),
        ..color
    };
    fill_circle(bgra, cx, cy, radius, fill);
    stroke_circle(
        bgra,
        cx,
        cy,
        radius + 1,
        Rgba {
            red: 255,
            green: 255,
            blue: 255,
            alpha: 230,
        },
    );
    stroke_circle(
        bgra,
        cx,
        cy,
        radius + 2,
        Rgba {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 200,
        },
    );
}

fn draw_badge_circle(bgra: &mut [u8], cx: i32, cy: i32, radius: i32) {
    stroke_circle(bgra, cx, cy, radius, badge_white());
    stroke_circle(bgra, cx, cy, radius + 1, badge_black());
}

fn draw_badge_rect(bgra: &mut [u8], left: i32, top: i32, w: i32, h: i32) {
    stroke_rect(bgra, left, top, w, h, badge_white());
    stroke_rect(bgra, left - 1, top - 1, w + 2, h + 2, badge_black());
}

/// 직선 도구: 대각선 배지
fn draw_badge_line(bgra: &mut [u8], left: i32, top: i32, len: i32) {
    let x0 = left;
    let y0 = top + len;
    let x1 = left + len;
    let y1 = top;
    draw_line_px(bgra, x0 - 1, y0, x1 - 1, y1, badge_black());
    draw_line_px(bgra, x0 + 1, y0, x1 + 1, y1, badge_black());
    draw_line_px(bgra, x0, y0 - 1, x1, y1 - 1, badge_black());
    draw_line_px(bgra, x0, y0 + 1, x1, y1 + 1, badge_black());
    draw_line_px(bgra, x0, y0, x1, y1, badge_white());
}

/// 지우개: 작은 직사각 + 빗금
fn draw_badge_eraser(bgra: &mut [u8], left: i32, top: i32) {
    let w = 11;
    let h = 8;
    // 본체
    for y in top..(top + h) {
        for x in left..(left + w) {
            put_bgra(bgra, x, y, 255, 255, 255, 255);
        }
    }
    stroke_rect(bgra, left - 1, top - 1, w + 2, h + 2, badge_black());
    // 끝쪽 어두운 팁
    for y in top..(top + h) {
        for x in (left + w - 3)..(left + w) {
            put_bgra(bgra, x, y, 80, 80, 80, 255);
        }
    }
    // 빗금 (지우개 표시)
    draw_line_px(bgra, left + 2, top + h - 2, left + w - 4, top + 1, badge_black());
    draw_line_px(bgra, left + 4, top + h - 2, left + w - 2, top + 1, badge_black());
}

fn draw_badge_up_arrow(bgra: &mut [u8], cx: i32, top: i32, size: i32) {
    // 위를 향한 화살표 (▲)
    let tip = (cx, top);
    let bl = (cx - size / 2, top + size);
    let br = (cx + size / 2, top + size);
    fill_triangle(bgra, tip, bl, br, badge_white());
    stroke_triangle(bgra, tip, bl, br, badge_black());
}

fn draw_badge_blur_text(bgra: &mut [u8], left: i32, top: i32) {
    // 5×7 픽셀 폰트 「Blur」
    const GLYPH_W: i32 = 5;
    const GLYPH_H: i32 = 7;
    const GAP: i32 = 1;
    let glyphs: &[&[u8]] = &[
        // B
        &[0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        // l
        &[0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01110],
        // u
        &[0b00000, 0b00000, 0b10001, 0b10001, 0b10001, 0b10001, 0b01111],
        // r
        &[0b00000, 0b00000, 0b10110, 0b11001, 0b10000, 0b10000, 0b10000],
    ];
    let mut x = left;
    for rows in glyphs {
        for (row_i, &bits) in rows.iter().enumerate() {
            for col in 0..GLYPH_W {
                if (bits >> (GLYPH_W - 1 - col)) & 1 == 0 {
                    continue;
                }
                let px = x + col;
                let py = top + row_i as i32;
                // 검은 외곽
                for oy in -1..=1 {
                    for ox in -1..=1 {
                        if ox == 0 && oy == 0 {
                            continue;
                        }
                        put_bgra(bgra, px + ox, py + oy, 0, 0, 0, 220);
                    }
                }
                put_bgra(bgra, px, py, 255, 255, 255, 255);
            }
        }
        x += GLYPH_W + GAP;
    }
    let _ = GLYPH_H;
}

fn badge_white() -> Rgba {
    Rgba {
        red: 255,
        green: 255,
        blue: 255,
        alpha: 255,
    }
}

fn badge_black() -> Rgba {
    Rgba {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 220,
    }
}

fn fill_circle(bgra: &mut [u8], cx: i32, cy: i32, radius: i32, color: Rgba) {
    let r2 = radius * radius;
    for y in (cy - radius)..=(cy + radius) {
        for x in (cx - radius)..=(cx + radius) {
            let dx = x - cx;
            let dy = y - cy;
            if dx * dx + dy * dy <= r2 {
                blend_bgra(bgra, x, y, color);
            }
        }
    }
}

fn stroke_circle(bgra: &mut [u8], cx: i32, cy: i32, radius: i32, color: Rgba) {
    let outer = radius * radius;
    let inner = ((radius - 1).max(0)) * ((radius - 1).max(0));
    for y in (cy - radius)..=(cy + radius) {
        for x in (cx - radius)..=(cx + radius) {
            let d = (x - cx) * (x - cx) + (y - cy) * (y - cy);
            if d <= outer && d >= inner {
                blend_bgra(bgra, x, y, color);
            }
        }
    }
}

fn stroke_rect(bgra: &mut [u8], left: i32, top: i32, w: i32, h: i32, color: Rgba) {
    let right = left + w - 1;
    let bottom = top + h - 1;
    for x in left..=right {
        blend_bgra(bgra, x, top, color);
        blend_bgra(bgra, x, bottom, color);
    }
    for y in top..=bottom {
        blend_bgra(bgra, left, y, color);
        blend_bgra(bgra, right, y, color);
    }
}

fn fill_triangle(
    bgra: &mut [u8],
    a: (i32, i32),
    b: (i32, i32),
    c: (i32, i32),
    color: Rgba,
) {
    let min_x = a.0.min(b.0).min(c.0);
    let max_x = a.0.max(b.0).max(c.0);
    let min_y = a.1.min(b.1).min(c.1);
    let max_y = a.1.max(b.1).max(c.1);
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            if point_in_triangle(x, y, a, b, c) {
                blend_bgra(bgra, x, y, color);
            }
        }
    }
}

fn stroke_triangle(
    bgra: &mut [u8],
    a: (i32, i32),
    b: (i32, i32),
    c: (i32, i32),
    color: Rgba,
) {
    draw_line_px(bgra, a.0, a.1, b.0, b.1, color);
    draw_line_px(bgra, b.0, b.1, c.0, c.1, color);
    draw_line_px(bgra, c.0, c.1, a.0, a.1, color);
}

fn point_in_triangle(px: i32, py: i32, a: (i32, i32), b: (i32, i32), c: (i32, i32)) -> bool {
    let sign = |p1: (i32, i32), p2: (i32, i32), p3: (i32, i32)| {
        (p1.0 - p3.0) * (p2.1 - p3.1) - (p2.0 - p3.0) * (p1.1 - p3.1)
    };
    let b1 = sign((px, py), a, b) < 0;
    let b2 = sign((px, py), b, c) < 0;
    let b3 = sign((px, py), c, a) < 0;
    b1 == b2 && b2 == b3
}

fn draw_line_px(bgra: &mut [u8], x0: i32, y0: i32, x1: i32, y1: i32, color: Rgba) {
    let mut x = x0;
    let mut y = y0;
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        blend_bgra(bgra, x, y, color);
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

fn put_bgra(bgra: &mut [u8], x: i32, y: i32, r: u8, g: u8, b: u8, a: u8) {
    if x < 0 || y < 0 || x >= CURSOR_SIZE || y >= CURSOR_SIZE {
        return;
    }
    let i = ((y * CURSOR_SIZE + x) * 4) as usize;
    bgra[i] = b;
    bgra[i + 1] = g;
    bgra[i + 2] = r;
    bgra[i + 3] = a;
}

fn blend_bgra(bgra: &mut [u8], x: i32, y: i32, color: Rgba) {
    if x < 0 || y < 0 || x >= CURSOR_SIZE || y >= CURSOR_SIZE {
        return;
    }
    let i = ((y * CURSOR_SIZE + x) * 4) as usize;
    let src_a = color.alpha as u32;
    let inv = 255 - src_a;
    bgra[i] = ((color.blue as u32 * src_a + bgra[i] as u32 * inv) / 255) as u8;
    bgra[i + 1] = ((color.green as u32 * src_a + bgra[i + 1] as u32 * inv) / 255) as u8;
    bgra[i + 2] = ((color.red as u32 * src_a + bgra[i + 2] as u32 * inv) / 255) as u8;
    bgra[i + 3] = (bgra[i + 3] as u32).max(src_a) as u8;
}

unsafe fn cursor_from_bgra(bgra: &[u8], size: i32, hot_x: i32, hot_y: i32) -> Option<HCURSOR> {
    let screen_dc = GetDC(null_mut());
    if screen_dc.is_null() {
        return None;
    }
    let (color_bmp, bits) = create_dib(screen_dc, size, size);
    if color_bmp.is_null() || bits.is_null() {
        ReleaseDC(null_mut(), screen_dc);
        return None;
    }
    let byte_count = (size * size * 4) as usize;
    std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits as *mut u8, byte_count);

    // AND 마스크: 알파 낮은 픽셀 = 투명(1)
    let mask_stride = ((size as usize + 31) / 32) * 4;
    let mut mask = vec![0u8; mask_stride * size as usize];
    for y in 0..size {
        for x in 0..size {
            let a = bgra[((y * size + x) * 4 + 3) as usize];
            if a < 16 {
                let bit = 7 - (x as usize % 8);
                mask[y as usize * mask_stride + x as usize / 8] |= 1 << bit;
            }
        }
    }
    let mask_bmp = CreateBitmap(size, size, 1, 1, mask.as_ptr() as *const c_void);
    if mask_bmp.is_null() {
        DeleteObject(color_bmp);
        ReleaseDC(null_mut(), screen_dc);
        return None;
    }

    let info = ICONINFO {
        fIcon: 0,
        xHotspot: hot_x as u32,
        yHotspot: hot_y as u32,
        hbmMask: mask_bmp,
        hbmColor: color_bmp,
    };
    let cursor = CreateIconIndirect(&info);
    DeleteObject(color_bmp);
    DeleteObject(mask_bmp);
    ReleaseDC(null_mut(), screen_dc);
    if cursor.is_null() {
        None
    } else {
        Some(cursor)
    }
}
