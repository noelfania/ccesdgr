use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CombineRgn, CreateRectRgn, CreateSolidBrush, DeleteObject, EndPaint, FillRect,
    FrameRect, InvalidateRect, MonitorFromRect, SelectObject, SetBkMode, SetTextColor, SetWindowRgn, TextOutW,
    UpdateWindow, MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, RGN_DIFF, RGN_OR,
};
use windows_sys::Win32::Graphics::Dwm::DwmFlush;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, ReleaseCapture, SetCapture, SetFocus,
};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetCursorPos, GetSystemMetrics, GetWindowRect,
    KillTimer, LoadCursorW, MessageBoxW, RegisterClassExW, SendMessageW, SetCursor, SetTimer, SetWindowPos,
    SetWindowTextW, ShowWindow, BS_PUSHBUTTON, HMENU, HTCAPTION, HTCLIENT, IDC_ARROW, IDC_SIZENESW,
    IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE, SM_CYVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, SWP_NOACTIVATE, SWP_NOZORDER, SW_HIDE, SW_SHOW, SW_SHOWDEFAULT, WM_DPICHANGED, WINDOW_STYLE,
    IDYES, MB_ICONQUESTION, MB_YESNO, WM_CAPTURECHANGED,
    WM_COMMAND, WM_CREATE, WM_DESTROY, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_MOVE, WM_NCHITTEST, WM_PAINT, WM_SETCURSOR, WM_SETFONT, WM_TIMER, WNDCLASSEXW, WS_CHILD,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_TABSTOP, WS_VISIBLE, MB_ICONERROR, MB_OK,
};

use crate::config::{AppConfig, CaptureBarPos, CaptureFrameConfig};
use crate::win;

// ── 콤보박스 상수 (windows-sys에서 직접 임포트 불가한 항목) ───────────────
const CBS_DROPDOWNLIST: WINDOW_STYLE = 3;
const CB_ADDSTRING: u32 = 0x0143;
const CB_SETCURSEL: u32 = 0x014E;
const CB_GETCURSEL: u32 = 0x0147;
const CBN_SELCHANGE: i32 = 1;

// ── 레이아웃 상수 ─────────────────────────────────────────────────────────
const CLASS_NAME: &str = "GMTLCaptureUI";

// 아래 치수는 96 DPI 기준 — 사용할 때 sc()로 현재 모니터 DPI에 맞춘다. BORDER는 캡처 영역 계산에 쓰여 1px 고정.
const BAR_H: i32 = 32; // 바 높이
const BORDER: i32 = 1; // 캡처 프레임 테두리 두께 (1px 헤어라인)
const STRIPE_LEN: i32 = 4; // 흰/검(또는 적/검) 대시 길이 — thickness와 분리 (marching ants)
const SHADOW: i32 = 0; // 프레임 외부 그림자 없음
const GRIP: i32 = 12; // 프레임 안쪽 리사이즈 핸들 한 변 (녹화 중에는 숨겨 GIF에 찍히지 않는다)
const MIN_SEL_W: i32 = 320; // 최소 캡처 너비
const MIN_SEL_H: i32 = 100; // 최소 캡처 높이
const TIMER_POLL: usize = 1;

// ── 컨트롤 ID ────────────────────────────────────────────────────────────
const ID_TOGGLE: i32 = 3001;
const ID_CANCEL: i32 = 3003;
const ID_BAR_POS: i32 = 3004;
const ID_PRESET_COMBO: i32 = 3005;
const ID_RATIO_COMBO: i32 = 3006;

// ── 버튼/콤보 치수 ─────────────────────────────────────────────────────────
const BTN_W: i32 = 26;
const BTN_H: i32 = 26;
const COMBO_H: i32 = 22;
const COMBO_DROPDOWN_EXTRA: i32 = 200;
const COMBO_W: i32 = 65;
const PRESET_W: i32 = 85;
const CTRL_GAP: i32 = 4;
const BAR_TEXT_X: i32 = 6;
const BAR_TEXT_Y: i32 = 8;
/// 크기·경과 시간 문구에 남길 너비 — 부족하면 콤보를 왼쪽부터 숨긴다.
const BAR_TEXT_W: i32 = 150;

// ── 타입 정의 ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum BarPos {
    Top,
    Bottom,
}

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    None,
    N,
    S,
    E,
    W,
    NW,
    NE,
    SW,
    SE,
}

struct State {
    sel: RECT,
    drag: Option<DragInfo>,
    toggle_btn: isize,
    cancel_btn: isize,
    bar_pos_ctrl: isize,
    preset_ctrl: isize,
    ratio_ctrl: isize,
    tooltip: isize,
    is_stopping: bool,
    cancelled: bool,
    /// 녹화를 시작했고 poll_done 마무리가 아직 안 돈 상태 (인코더 종료 직후 틈 포함)
    finish_pending: bool,
    /// 토글 ON일 때 Save As 기본 파일명
    suggested_gif_name: String,
    config: AppConfig,
    aspect: Option<(i32, i32)>,
    bar_pos: BarPos,
}

struct DragInfo {
    mode: Drag,
    start_cx: i32,
    start_cy: i32,
    start_sel: RECT,
}

/// 창 내 좌표 기준으로 캡처 프레임 좌상단 위치
struct WinGeom {
    win_w: i32,
    win_h: i32,
    // 바 영역 (창 좌표)
    bar_l: i32,
    bar_t: i32,
    bar_r: i32,
    bar_b: i32,
    // 캡처 프레임 시작점 (창 좌표)
    fx: i32,
    fy: i32,
}

fn sc(value: i32) -> i32 {
    win::scale(value, DPI.load(Ordering::Relaxed))
}

fn bar_h() -> i32 {
    sc(BAR_H)
}

fn min_sel_w() -> i32 {
    sc(MIN_SEL_W)
}

fn min_sel_h() -> i32 {
    sc(MIN_SEL_H)
}

/// DPI 저장 + 해당 DPI용 툴바 폰트 교체. 이전 폰트를 돌려주며, 컨트롤을 다시 만든 뒤 호출자가 지운다.
unsafe fn set_dpi(dpi: u32) -> isize {
    let dpi = if dpi == 0 { 96 } else { dpi };
    DPI.store(dpi, Ordering::Relaxed);
    UI_FONT.swap(win::create_ui_font(dpi) as isize, Ordering::Relaxed)
}

/// 프레임이 놓일 모니터의 DPI (창을 만들기 전 크기 계산용)
unsafe fn dpi_for_rect(rect: &RECT) -> u32 {
    let monitor = MonitorFromRect(rect, MONITOR_DEFAULTTONEAREST);
    let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
    if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) < 0 {
        return 96;
    }
    dpi_x
}

struct ToolbarLayout {
    btn_w: i32,
    btn_h: i32,
    combo_w: i32,
    preset_w: i32,
    btn_y: i32,
    combo_y: i32,
    cancel_x: i32,
    toggle_x: i32,
    bar_pos_x: i32,
    ratio_x: i32,
    preset_x: i32,
    show_bar_pos: bool,
    show_ratio: bool,
    show_preset: bool,
}

fn toolbar_layout(win_w: i32, bar_t: i32, dpi: u32) -> ToolbarLayout {
    let scale = |value| win::scale(value, dpi);
    let gap = scale(CTRL_GAP);
    let btn_w = scale(BTN_W);
    let combo_w = scale(COMBO_W);
    let preset_w = scale(PRESET_W);
    let cancel_x = win_w - gap - btn_w;
    let toggle_x = cancel_x - gap - btn_w;
    let bar_pos_x = toggle_x - gap - combo_w;
    let ratio_x = bar_pos_x - gap - combo_w;
    let preset_x = ratio_x - gap - preset_w;
    let text_end = scale(BAR_TEXT_X + BAR_TEXT_W);
    ToolbarLayout {
        btn_w,
        btn_h: scale(BTN_H),
        combo_w,
        preset_w,
        btn_y: bar_t + (scale(BAR_H) - scale(BTN_H)) / 2,
        combo_y: bar_t + (scale(BAR_H) - scale(COMBO_H)) / 2,
        cancel_x,
        toggle_x,
        bar_pos_x,
        ratio_x,
        preset_x,
        show_bar_pos: bar_pos_x >= text_end,
        show_ratio: ratio_x >= text_end,
        show_preset: preset_x >= text_end,
    }
}

/// 프레임 안쪽 리사이즈 핸들: 네 모서리 + 네 변 중앙 (창 좌표)
fn grip_rects(fx: i32, fy: i32, sel_w: i32, sel_h: i32, grip: i32) -> [RECT; 8] {
    let (left, mid_x, right) = (fx, fx + (sel_w - grip) / 2, fx + sel_w - grip);
    let (top, mid_y, bottom) = (fy, fy + (sel_h - grip) / 2, fy + sel_h - grip);
    [
        (left, top),
        (mid_x, top),
        (right, top),
        (left, mid_y),
        (right, mid_y),
        (left, bottom),
        (mid_x, bottom),
        (right, bottom),
    ]
    .map(|(x, y)| RECT {
        left: x,
        top: y,
        right: x + grip,
        bottom: y + grip,
    })
}

/// 녹화 중이거나 마무리 전이면 핸들을 숨긴다.
fn grips_visible() -> bool {
    STATE
        .lock()
        .expect("capture mutex poisoned")
        .as_ref()
        .is_some_and(|s| !s.finish_pending)
}

fn compute_geom(sel_w: i32, sel_h: i32, bar_pos: BarPos) -> WinGeom {
    match bar_pos {
        BarPos::Top => WinGeom {
            win_w: sel_w + 2 * SHADOW,
            win_h: bar_h() + sel_h + SHADOW,
            bar_l: 0,
            bar_t: 0,
            bar_r: sel_w + 2 * SHADOW,
            bar_b: bar_h(),
            fx: SHADOW,
            fy: bar_h(),
        },
        BarPos::Bottom => WinGeom {
            win_w: sel_w + 2 * SHADOW,
            win_h: SHADOW + sel_h + bar_h(),
            bar_l: 0,
            bar_t: SHADOW + sel_h,
            bar_r: sel_w + 2 * SHADOW,
            bar_b: SHADOW + sel_h + bar_h(),
            fx: SHADOW,
            fy: SHADOW,
        },
    }
}

fn win_pos(sel: &RECT, bar_pos: BarPos) -> (i32, i32) {
    match bar_pos {
        BarPos::Top => (sel.left - SHADOW, sel.top - bar_h()),
        BarPos::Bottom => (sel.left - SHADOW, sel.top - SHADOW),
    }
}

fn bar_pos_from_config(pos: CaptureBarPos) -> BarPos {
    match pos {
        CaptureBarPos::Top => BarPos::Top,
        CaptureBarPos::Bottom => BarPos::Bottom,
    }
}

fn bar_pos_to_config(pos: BarPos) -> CaptureBarPos {
    match pos {
        BarPos::Top => CaptureBarPos::Top,
        BarPos::Bottom => CaptureBarPos::Bottom,
    }
}

fn default_capture_sel() -> (RECT, BarPos) {
    let primary = win::primary_screen_rect();
    let sel_w = 640_i32
        .min(win::rect_width(&primary) - 100)
        .max(min_sel_w());
    let sel_h = 480_i32
        .min(win::rect_height(&primary) - 200)
        .max(min_sel_h());
    let cx = (primary.left + primary.right) / 2;
    let cy = (primary.top + primary.bottom) / 2;
    (
        RECT {
            left: cx - sel_w / 2,
            top: cy - sel_h / 2,
            right: cx + sel_w / 2,
            bottom: cy + sel_h / 2,
        },
        BarPos::Top,
    )
}

fn resolve_initial_sel(config: &AppConfig) -> (RECT, BarPos) {
    let default = default_capture_sel();
    let Some(saved) = &config.capture_frame else {
        return default;
    };

    let mut sel = saved.clone().to_rect();
    let bar_pos = bar_pos_from_config(saved.bar_pos);
    let virtual_screen = win::virtual_screen_rect();
    if win::intersect_rect(sel, virtual_screen).is_none() {
        return default;
    }
    if !move_sel_into_screen(&mut sel) {
        return default;
    }
    (sel, bar_pos)
}

/// 프레임을 자르지 않고 크기를 유지한 채 화면 안으로 옮긴다 (이동·프리셋용).
fn move_sel_into_screen(sel: &mut RECT) -> bool {
    *sel = win::shift_rect_into(*sel, win::virtual_screen_rect());
    normalize_sel(sel)
}

/// 툴바(BAR_H)가 들어갈 자리를 남긴 프레임 최대 높이 — 넘으면 툴바가 화면 밖으로 나간다.
fn max_sel_height(screen_h: i32) -> i32 {
    (screen_h - bar_h()).max(min_sel_h())
}

/// sel을 가상 화면 안으로 맞추고 최소 크기를 보장한다.
fn normalize_sel(sel: &mut RECT) -> bool {
    let screen = win::virtual_screen_rect();
    if !win::clamp_rect_to_virtual_screen(sel) {
        return false;
    }

    if win::rect_width(sel) < min_sel_w() {
        sel.right = (sel.left + min_sel_w()).min(screen.right);
        sel.left = sel.right - min_sel_w();
        if sel.left < screen.left {
            sel.left = screen.left;
            sel.right = sel.left + min_sel_w();
        }
    }
    if win::rect_height(sel) < min_sel_h() {
        sel.bottom = (sel.top + min_sel_h()).min(screen.bottom);
        sel.top = sel.bottom - min_sel_h();
        if sel.top < screen.top {
            sel.top = screen.top;
            sel.bottom = sel.top + min_sel_h();
        }
    }
    let max_h = max_sel_height(win::rect_height(&screen));
    if win::rect_height(sel) > max_h {
        sel.bottom = sel.top + max_h;
    }

    win::clamp_rect_to_virtual_screen(sel)
        && win::rect_width(sel) >= min_sel_w()
        && win::rect_height(sel) >= min_sel_h()
}

fn persist_capture_frame_from_state() {
    let snapshot = STATE.lock().expect("capture mutex poisoned");
    let Some(state) = snapshot.as_ref() else {
        return;
    };
    let frame = CaptureFrameConfig::from_rect(state.sel, bar_pos_to_config(state.bar_pos));
    crate::save_capture_frame(frame);
}

// ── 정적 상태 ────────────────────────────────────────────────────────────
static CAPTURE_HWND: AtomicIsize = AtomicIsize::new(0);
static STATE: Mutex<Option<State>> = Mutex::new(None);
/// 녹화 마무리(저장 대화상자·오류 창) 진행 중 — 이때는 새 작업과 종료를 막는다.
static SAVING: AtomicBool = AtomicBool::new(false);
static DPI: AtomicU32 = AtomicU32::new(96);
static UI_FONT: AtomicIsize = AtomicIsize::new(0);

/// 앱 종료 시 진행 중이던 녹화의 처리 결과
pub enum ExitOutcome {
    Nothing,
    Saved(std::path::PathBuf),
    Problem(String),
}

// ═══════════════════════════════════════════════════════════════════════════
// 공개 함수
// ═══════════════════════════════════════════════════════════════════════════

pub fn show(owner: HWND, config: AppConfig) {
    let existing = CAPTURE_HWND.load(Ordering::Relaxed);
    if existing != 0 {
        unsafe {
            ShowWindow(existing as HWND, SW_SHOW);
            SetFocus(existing as HWND);
        }
        return;
    }

    let instance = crate::win::module_handle();
    let class_name = win::to_wide(CLASS_NAME);
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(capture_proc),
        hInstance: instance,
        hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
        lpszClassName: class_name.as_ptr(),
        ..unsafe { zeroed() }
    };
    unsafe {
        RegisterClassExW(&class);
    }

    // 최소 크기 계산이 DPI를 쓰므로 저장된 프레임 위치의 모니터 기준으로 먼저 맞춘다.
    let dpi_probe = config
        .capture_frame
        .clone()
        .map(|frame| frame.to_rect())
        .unwrap_or_else(win::primary_screen_rect);
    let old_font = unsafe { set_dpi(dpi_for_rect(&dpi_probe)) };
    if old_font != 0 {
        unsafe {
            DeleteObject(old_font as _);
        }
    }

    let (sel, bar_pos) = resolve_initial_sel(&config);
    let sel_w = sel.right - sel.left;
    let sel_h = sel.bottom - sel.top;
    let geom = compute_geom(sel_w, sel_h, bar_pos);
    let (wx, wy) = win_pos(&sel, bar_pos);

    *STATE.lock().expect("capture mutex poisoned") = Some(State {
        sel,
        drag: None,
        toggle_btn: 0,
        cancel_btn: 0,
        bar_pos_ctrl: 0,
        preset_ctrl: 0,
        ratio_ctrl: 0,
        tooltip: 0,
        is_stopping: false,
        cancelled: false,
        finish_pending: false,
        suggested_gif_name: String::new(),
        config,
        aspect: None,
        bar_pos,
    });

    let title = win::to_wide("GIF Recording");
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            wx,
            wy,
            geom.win_w,
            geom.win_h,
            owner,
            null_mut(),
            instance,
            null_mut(),
        )
    };
    if !hwnd.is_null() {
        CAPTURE_HWND.store(hwnd as isize, Ordering::Relaxed);
        unsafe {
            ShowWindow(hwnd, SW_SHOW);
            SetFocus(hwnd);
        }
    }
}

/// 트레이/핫키에서 녹화 중지 요청 — 버튼의 ■와 같은 경로로 저장 흐름을 이어간다.
pub fn request_stop() {
    let hwnd = CAPTURE_HWND.load(Ordering::Relaxed);
    if hwnd == 0 {
        crate::recorder::stop();
        return;
    }
    unsafe { on_stop(hwnd as HWND) };
}

pub fn is_open() -> bool {
    CAPTURE_HWND.load(Ordering::Relaxed) != 0
}

/// 프레임이 열려 있고 녹화 전이면 녹화를 시작한다 (Ctrl+Alt+Shift+R 두 번째 누름).
pub fn start_recording() {
    let hwnd = CAPTURE_HWND.load(Ordering::Relaxed);
    if hwnd != 0 && !crate::recorder::is_recording() {
        unsafe { on_record(hwnd as HWND) };
    }
}

pub fn is_saving() -> bool {
    SAVING.load(Ordering::Relaxed)
}

/// 종료 직전 호출 — poll_done이 더 돌지 않으므로 진행 중이던 녹화 파일을 여기서 마무리한다.
/// `encoder_finished`가 false면 파일이 잘렸을 수 있다.
pub fn finalize_on_exit(encoder_finished: bool) -> ExitOutcome {
    let (pending, cancelled, ask_save, folder) = {
        let g = STATE.lock().expect("capture mutex poisoned");
        match g.as_ref() {
            Some(s) => (
                s.finish_pending,
                s.cancelled,
                s.config.ask_gif_save_path,
                std::path::PathBuf::from(&s.config.gif_output_path),
            ),
            None => return ExitOutcome::Nothing,
        }
    };
    if !pending {
        return ExitOutcome::Nothing;
    }

    let output_path = crate::recorder::last_output_path();
    if !encoder_finished {
        return ExitOutcome::Problem(format!(
            "The GIF encoder did not finish in time. The recording may be incomplete:\n{}",
            output_path.display()
        ));
    }
    if cancelled {
        let _ = std::fs::remove_file(&output_path);
        return ExitOutcome::Nothing;
    }
    if let Some(message) = crate::recorder::take_last_error() {
        let _ = std::fs::remove_file(&output_path);
        return ExitOutcome::Problem(format!("GIF recording failed.\n{message}"));
    }
    if crate::recorder::last_frame_count() == 0 {
        let _ = std::fs::remove_file(&output_path);
        return ExitOutcome::Nothing;
    }
    if !ask_save {
        // 이미 저장 폴더에 있다.
        return ExitOutcome::Nothing;
    }

    // 저장 위치를 물을 수 없으므로 임시 폴더 대신 기본 저장 폴더로 옮긴다.
    let moved = output_path
        .file_name()
        .map(|name| name.to_string_lossy().trim_start_matches("ccesdgr-").to_string())
        .ok_or_else(|| "invalid temporary file name".to_string())
        .and_then(|name| {
            std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
            let dest = folder.join(name);
            move_file(&output_path, &dest).map_err(|error| error.to_string())?;
            Ok(dest)
        });
    match moved {
        Ok(dest) => ExitOutcome::Saved(dest),
        Err(error) => ExitOutcome::Problem(format!(
            "Failed to move the recording to {}: {error}\nIt is kept at:\n{}",
            folder.display(),
            output_path.display()
        )),
    }
}

/// 볼륨이 달라 rename이 실패하면 복사 후 원본을 지운다.
fn move_file(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to)?;
    let _ = std::fs::remove_file(from);
    Ok(())
}

pub fn close() {
    let hwnd = CAPTURE_HWND.load(Ordering::Relaxed);
    if hwnd != 0 {
        unsafe {
            DestroyWindow(hwnd as HWND);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 메인 창 프로시저
// ═══════════════════════════════════════════════════════════════════════════

unsafe extern "system" fn capture_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            create_controls(hwnd);
            apply_region(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        // 다른 배율의 모니터로 이동: 프레임 크기는 실제 녹화 픽셀이므로 유지하고 툴바만 다시 맞춘다.
        WM_DPICHANGED => {
            let old_font = set_dpi(win::low_word(wp) as u32);
            reposition_window(hwnd);
            if old_font != 0 {
                DeleteObject(old_font as _);
            }
            0
        }
        WM_NCHITTEST => {
            let mut wr: RECT = zeroed();
            GetWindowRect(hwnd, &mut wr);
            let my = ((lp as usize >> 16) & 0xffff) as i16 as i32;
            let mx = (lp as usize & 0xffff) as i16 as i32;
            let wy = my - wr.top;
            let wx = mx - wr.left;
            let (sel_w, sel_h, bar_pos) = read_sel_dims_pos();
            let geom = compute_geom(sel_w, sel_h, bar_pos);
            if wx >= geom.bar_l && wx < geom.bar_r && wy >= geom.bar_t && wy < geom.bar_b {
                if crate::recorder::is_recording() {
                    return HTCLIENT as LRESULT;
                }
                return HTCAPTION as LRESULT;
            }
            HTCLIENT as LRESULT
        }
        WM_SETCURSOR => {
            let ht = win::low_word(lp as usize) as i32;
            if ht == HTCLIENT as i32 && crate::recorder::is_recording() {
                // 녹화 중에는 크기를 바꿀 수 없다.
                SetCursor(LoadCursorW(null_mut(), IDC_ARROW));
                return 1;
            }
            if ht == HTCLIENT as i32 {
                let mut pt = POINT { x: 0, y: 0 };
                GetCursorPos(&mut pt);
                let mut wr: RECT = zeroed();
                GetWindowRect(hwnd, &mut wr);
                let wx = pt.x - wr.left;
                let wy = pt.y - wr.top;
                let (sel_w, sel_h, bar_pos) = read_sel_dims_pos();
                let geom = compute_geom(sel_w, sel_h, bar_pos);
                // 바 영역은 HTCAPTION이라 SETCURSOR가 오지 않음
                let mode = hit_test_resize(wx, wy, geom.fx, geom.fy, sel_w, sel_h);
                SetCursor(LoadCursorW(null_mut(), cursor_for_drag(mode)));
                return 1;
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_MOVE => {
            let nx = (lp as usize & 0xffff) as i16 as i32;
            let ny = ((lp as usize >> 16) & 0xffff) as i16 as i32;
            let mut g = STATE.lock().expect("capture mutex poisoned");
            let mut pushed_back = false;
            if let Some(s) = g.as_mut() {
                let w = s.sel.right - s.sel.left;
                let h = s.sel.bottom - s.sel.top;
                let geom = compute_geom(w, h, s.bar_pos);
                s.sel.left = nx + geom.fx;
                s.sel.top = ny + geom.fy;
                s.sel.right = s.sel.left + w;
                s.sel.bottom = s.sel.top + h;
                let dragged_to = s.sel;
                move_sel_into_screen(&mut s.sel);
                // 창도 함께 되돌려 표시된 프레임과 실제 녹화 영역을 일치시킨다.
                pushed_back = s.sel.left != dragged_to.left || s.sel.top != dragged_to.top;
            }
            drop(g);
            if auto_adjust_bar_pos() {
                reposition_window(hwnd);
            } else if pushed_back {
                refresh_capture_window(hwnd);
            }
            0
        }
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_LBUTTONDOWN => {
            on_mouse_down(hwnd, lp);
            0
        }
        WM_MOUSEMOVE => {
            on_mouse_move(hwnd);
            0
        }
        WM_LBUTTONUP => {
            on_mouse_up();
            0
        }
        // 다른 창이 캡처를 가져가면 버튼을 뗀 이벤트가 오지 않아 드래그를 끝낸다.
        WM_CAPTURECHANGED => {
            if let Some(s) = STATE.lock().expect("capture mutex poisoned").as_mut() {
                s.drag = None;
            }
            0
        }
        WM_COMMAND => {
            let id = win::low_word(wp as usize) as i32;
            let note = ((wp as usize >> 16) & 0xffff) as i32;
            match id {
                ID_TOGGLE => {
                    if note == 0 {
                        on_toggle(hwnd);
                    }
                    0
                }
                ID_CANCEL => {
                    if note == 0 {
                        on_cancel(hwnd);
                    }
                    0
                }
                ID_BAR_POS if note == CBN_SELCHANGE => {
                    on_bar_pos_changed(hwnd);
                    0
                }
                ID_PRESET_COMBO if note == CBN_SELCHANGE => {
                    on_preset_changed(hwnd);
                    0
                }
                ID_RATIO_COMBO if note == CBN_SELCHANGE => {
                    on_ratio_changed(hwnd);
                    0
                }
                _ => DefWindowProcW(hwnd, msg, wp, lp),
            }
        }
        WM_TIMER => {
            if wp == TIMER_POLL {
                poll_done(hwnd);
            }
            0
        }
        WM_DESTROY => {
            persist_capture_frame_from_state();
            KillTimer(hwnd, TIMER_POLL);
            CAPTURE_HWND.store(0, Ordering::Relaxed);
            *STATE.lock().expect("capture mutex poisoned") = None;
            let font = UI_FONT.swap(0, Ordering::Relaxed);
            if font != 0 {
                DeleteObject(font as _);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 컨트롤 생성 / 재배치 / 제거
// ═══════════════════════════════════════════════════════════════════════════

unsafe fn create_controls(hwnd: HWND) {
    let (sel_w, sel_h, bar_pos, aspect, is_recording) = {
        let g = STATE.lock().expect("capture mutex poisoned");
        let s = g.as_ref().unwrap();
        (
            s.sel.right - s.sel.left,
            s.sel.bottom - s.sel.top,
            s.bar_pos,
            s.aspect,
            crate::recorder::is_recording(),
        )
    };
    let geom = compute_geom(sel_w, sel_h, bar_pos);
    let instance = crate::win::module_handle();

    let make_btn = |id: i32, label: &str, x: i32, y: i32, w: i32, h: i32| -> isize {
        let cls = win::to_wide("BUTTON");
        let txt = win::to_wide(label);
        let btn = CreateWindowExW(
            0,
            cls.as_ptr(),
            txt.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON as WINDOW_STYLE,
            x,
            y,
            w,
            h,
            hwnd,
            id as isize as HMENU,
            instance,
            null_mut(),
        );
        if !btn.is_null() {
            SendMessageW(btn, WM_SETFONT, UI_FONT.load(Ordering::Relaxed) as WPARAM, 1);
        }
        btn as isize
    };

    let make_combo = |id: i32, x: i32, y: i32, w: i32| -> isize {
        let cls = win::to_wide("COMBOBOX");
        let combo = CreateWindowExW(
            0,
            cls.as_ptr(),
            null(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | CBS_DROPDOWNLIST,
            x,
            y,
            w,
            sc(COMBO_H + COMBO_DROPDOWN_EXTRA),
            hwnd,
            id as isize as HMENU,
            instance,
            null_mut(),
        );
        if !combo.is_null() {
            SendMessageW(combo, WM_SETFONT, UI_FONT.load(Ordering::Relaxed) as WPARAM, 1);
        }
        combo as isize
    };

    let add_str = |combo: isize, s: &str| {
        let ws = win::to_wide(s);
        SendMessageW(combo as HWND, CB_ADDSTRING, 0, ws.as_ptr() as LPARAM);
    };

    let (toggle_btn, cancel_btn, bar_pos_ctrl, preset_ctrl, ratio_ctrl);

    // 위치·표시 여부는 끝에서 update_ctrl_positions가 맞춘다.
    let layout = toolbar_layout(geom.win_w, geom.bar_t, DPI.load(Ordering::Relaxed));
    let (btn_y, btn_h, btn_w) = (layout.btn_y, layout.btn_h, layout.btn_w);

    cancel_btn = make_btn(ID_CANCEL, "✕", layout.cancel_x, btn_y, btn_w, btn_h);
    toggle_btn = make_btn(ID_TOGGLE, "●", layout.toggle_x, btn_y, btn_w, btn_h);

    bar_pos_ctrl = make_combo(ID_BAR_POS, layout.bar_pos_x, layout.combo_y, layout.combo_w);
    ratio_ctrl = make_combo(ID_RATIO_COMBO, layout.ratio_x, layout.combo_y, layout.combo_w);
    preset_ctrl = make_combo(ID_PRESET_COMBO, layout.preset_x, layout.combo_y, layout.preset_w);

    add_str(bar_pos_ctrl, "Top");
    add_str(bar_pos_ctrl, "Bottom");
    let bp_idx = match bar_pos {
        BarPos::Top => 0,
        BarPos::Bottom => 1,
    };
    SendMessageW(bar_pos_ctrl as HWND, CB_SETCURSEL, bp_idx, 0);

    for s in ["Free", "1:1", "4:3", "16:9", "9:16"] {
        add_str(ratio_ctrl, s);
    }
    let ratio_idx = match aspect {
        None => 0,
        Some((1, 1)) => 1,
        Some((4, 3)) => 2,
        Some((16, 9)) => 3,
        Some((9, 16)) => 4,
        _ => 0,
    };
    SendMessageW(ratio_ctrl as HWND, CB_SETCURSEL, ratio_idx, 0);

    for s in [
        "Size",
        "640x480",
        "1280x720",
        "1920x1080",
        "1024x768",
        "800x600",
    ] {
        add_str(preset_ctrl, s);
    }
    SendMessageW(preset_ctrl as HWND, CB_SETCURSEL, 0, 0);

    if is_recording {
        let tw = win::to_wide("■");
        SetWindowTextW(toggle_btn as HWND, tw.as_ptr());
        // 녹화 영역은 시작 시 고정되므로 크기 변경 컨트롤을 잠근다.
        EnableWindow(preset_ctrl as HWND, 0);
        EnableWindow(ratio_ctrl as HWND, 0);
    }

    let tooltip = win::create_tooltip(hwnd);
    if !tooltip.is_null() {
        win::add_tooltip(
            tooltip,
            hwnd,
            toggle_btn as HWND,
            &format!(
                "Start / stop recording ({})",
                crate::hotkey_bindings().record.label()
            ),
        );
        win::add_tooltip(
            tooltip,
            hwnd,
            cancel_btn as HWND,
            "Close (while recording: discard)",
        );
        win::add_tooltip(tooltip, hwnd, preset_ctrl as HWND, "Capture size preset");
        win::add_tooltip(tooltip, hwnd, ratio_ctrl as HWND, "Lock aspect ratio");
        win::add_tooltip(tooltip, hwnd, bar_pos_ctrl as HWND, "Toolbar position");
    }

    {
        let mut g = STATE.lock().expect("capture mutex poisoned");
        if let Some(s) = g.as_mut() {
            s.tooltip = tooltip as isize;
            s.toggle_btn = toggle_btn;
            s.cancel_btn = cancel_btn;
            s.bar_pos_ctrl = bar_pos_ctrl;
            s.preset_ctrl = preset_ctrl;
            s.ratio_ctrl = ratio_ctrl;
        }
    }
    update_ctrl_positions(hwnd);
}

unsafe fn destroy_controls() {
    let (t, c, b, p, r, tip) = {
        let g = STATE.lock().expect("capture mutex poisoned");
        let s = match g.as_ref() {
            Some(s) => s,
            None => return,
        };
        (
            s.toggle_btn as HWND,
            s.cancel_btn as HWND,
            s.bar_pos_ctrl as HWND,
            s.preset_ctrl as HWND,
            s.ratio_ctrl as HWND,
            s.tooltip as HWND,
        )
    };
    for h in [t, c, b, p, r, tip] {
        if !h.is_null() {
            DestroyWindow(h);
        }
    }
    let mut g = STATE.lock().expect("capture mutex poisoned");
    if let Some(s) = g.as_mut() {
        s.toggle_btn = 0;
        s.cancel_btn = 0;
        s.bar_pos_ctrl = 0;
        s.preset_ctrl = 0;
        s.ratio_ctrl = 0;
        s.tooltip = 0;
    }
}

unsafe fn rebuild_controls(hwnd: HWND) {
    destroy_controls();
    create_controls(hwnd);
}

unsafe fn update_ctrl_positions(hwnd: HWND) {
    let (bar_pos, sel_w, sel_h, t, c, b, p, r) = {
        let g = STATE.lock().expect("capture mutex poisoned");
        let s = match g.as_ref() {
            Some(s) => s,
            None => return,
        };
        let w = s.sel.right - s.sel.left;
        let h = s.sel.bottom - s.sel.top;
        (
            s.bar_pos,
            w,
            h,
            s.toggle_btn as HWND,
            s.cancel_btn as HWND,
            s.bar_pos_ctrl as HWND,
            s.preset_ctrl as HWND,
            s.ratio_ctrl as HWND,
        )
    };

    let geom = compute_geom(sel_w, sel_h, bar_pos);
    let layout = toolbar_layout(geom.win_w, geom.bar_t, DPI.load(Ordering::Relaxed));
    let dropdown_h = sc(COMBO_H + COMBO_DROPDOWN_EXTRA);

    let fl = SWP_NOZORDER | SWP_NOACTIVATE;
    if !b.is_null() {
        let bp_idx = match bar_pos {
            BarPos::Top => 0,
            BarPos::Bottom => 1,
        };
        SendMessageW(b, CB_SETCURSEL, bp_idx, 0);
    }
    if !c.is_null() {
        SetWindowPos(c, null_mut(), layout.cancel_x, layout.btn_y, layout.btn_w, layout.btn_h, fl);
    }
    if !t.is_null() {
        SetWindowPos(t, null_mut(), layout.toggle_x, layout.btn_y, layout.btn_w, layout.btn_h, fl);
    }
    for (control, x, width, show) in [
        (b, layout.bar_pos_x, layout.combo_w, layout.show_bar_pos),
        (r, layout.ratio_x, layout.combo_w, layout.show_ratio),
        (p, layout.preset_x, layout.preset_w, layout.show_preset),
    ] {
        if !control.is_null() {
            SetWindowPos(control, null_mut(), x, layout.combo_y, width, dropdown_h, fl);
            ShowWindow(control, if show { SW_SHOW } else { SW_HIDE });
        }
    }
    _ = hwnd;
}

// ═══════════════════════════════════════════════════════════════════════════
// 창 모양 (SetWindowRgn)
// ═══════════════════════════════════════════════════════════════════════════

unsafe fn apply_region(hwnd: HWND) {
    let (sel_w, sel_h, bar_pos) = read_sel_dims_pos();
    let geom = compute_geom(sel_w, sel_h, bar_pos);

    // 바 영역
    let bar = CreateRectRgn(geom.bar_l, geom.bar_t, geom.bar_r, geom.bar_b);

    // 프레임 영역 — 바의 반대쪽
    let (fo_l, fo_t, fo_r, fo_b) = match bar_pos {
        BarPos::Top => (0, geom.bar_b, geom.win_w, geom.win_h),
        BarPos::Bottom => (0, 0, geom.win_w, geom.bar_t),
    };
    let frame_outer = CreateRectRgn(fo_l, fo_t, fo_r, fo_b);

    // 투명으로 뚫릴 내부
    let fi_l = geom.fx + BORDER;
    let fi_t = geom.fy + BORDER;
    let fi_r = geom.fx + sel_w - BORDER;
    let fi_b = geom.fy + sel_h - BORDER;
    let frame_inner = CreateRectRgn(fi_l, fi_t, fi_r, fi_b);

    CombineRgn(frame_outer, frame_outer, frame_inner, RGN_DIFF);
    DeleteObject(frame_inner);

    if grips_visible() {
        for grip in grip_rects(geom.fx, geom.fy, sel_w, sel_h, sc(GRIP)) {
            let grip_rgn = CreateRectRgn(grip.left, grip.top, grip.right, grip.bottom);
            CombineRgn(frame_outer, frame_outer, grip_rgn, RGN_OR);
            DeleteObject(grip_rgn);
        }
    }

    CombineRgn(bar, bar, frame_outer, RGN_OR);
    DeleteObject(frame_outer);

    SetWindowRgn(hwnd, bar, 1);
}

// ═══════════════════════════════════════════════════════════════════════════
// 페인트
// ═══════════════════════════════════════════════════════════════════════════

unsafe fn paint(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);

    let (sel_w, sel_h, bar_pos) = read_sel_dims_pos();
    let geom = compute_geom(sel_w, sel_h, bar_pos);
    let recording = crate::recorder::is_recording();

    // ── 바 배경 ─────────────────────────────────────────────────────────
    let bar_brush = CreateSolidBrush(win::colorref(40, 40, 40));
    FillRect(
        hdc,
        &RECT {
            left: geom.bar_l,
            top: geom.bar_t,
            right: geom.bar_r,
            bottom: geom.bar_b,
        },
        bar_brush,
    );
    DeleteObject(bar_brush);

    // ── 크기 텍스트 ─────────────────────────────────────────────────────
    SetBkMode(hdc, 1);
    SetTextColor(hdc, win::colorref(180, 180, 180));
    let old_font = SelectObject(hdc, UI_FONT.load(Ordering::Relaxed) as _);
    let even_size = STATE
        .lock()
        .expect("capture mutex poisoned")
        .as_ref()
        .map(|s| s.config.gif_even_size)
        .unwrap_or(true);
    let (cap_w, cap_h) = output_capture_size_from_frame(sel_w, sel_h, even_size);
    let size_text = match crate::recorder::elapsed() {
        Some(elapsed) => format!(
            "{}×{}   ● REC {}",
            cap_w,
            cap_h,
            crate::recorder::format_elapsed(elapsed)
        ),
        None => format!("{}×{}", cap_w, cap_h),
    };
    if recording {
        SetTextColor(hdc, win::colorref(255, 120, 120));
    }
    let size_text = win::to_wide(&size_text);
    TextOutW(
        hdc,
        geom.bar_l + sc(BAR_TEXT_X),
        geom.bar_t + sc(BAR_TEXT_Y),
        size_text.as_ptr(),
        (size_text.len() - 1) as i32,
    );
    SelectObject(hdc, old_font);

    let fx = geom.fx;
    let fy = geom.fy;

    // ── 프레임 테두리 (줄무늬) ───────────────────────────────────────────
    let (color_a, color_b) = if recording {
        (
            win::colorref(220, 50, 50),
            win::colorref(0, 0, 0),
        )
    } else {
        (
            win::colorref(255, 255, 255),
            win::colorref(0, 0, 0),
        )
    };
    paint_stripe_border(
        hdc,
        fx,
        fy,
        sel_w,
        sel_h,
        BORDER,
        STRIPE_LEN,
        color_a,
        color_b,
    );

    // ── 리사이즈 핸들 ───────────────────────────────────────────────────
    if grips_visible() {
        let fill = CreateSolidBrush(win::colorref(255, 255, 255));
        let outline = CreateSolidBrush(win::colorref(0, 0, 0));
        for grip in grip_rects(fx, fy, sel_w, sel_h, sc(GRIP)) {
            FillRect(hdc, &grip, fill);
            FrameRect(hdc, &grip, outline);
        }
        DeleteObject(fill);
        DeleteObject(outline);
    }

    EndPaint(hwnd, &ps);
}

/// 1px(또는 thickness) 헤어라인 위에 흰/검 대시를 그린다.
/// dash_len은 테두리 두께와 별개 — thickness를 dash로 쓰면 1px일 때 줄무늬가 안 보인다.
unsafe fn paint_stripe_border(
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    fx: i32,
    fy: i32,
    sel_w: i32,
    sel_h: i32,
    thickness: i32,
    dash_len: i32,
    color_a: windows_sys::Win32::Foundation::COLORREF,
    color_b: windows_sys::Win32::Foundation::COLORREF,
) {
    let t = thickness.max(1);
    let dash = dash_len.max(1);
    // 상·하
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
    // 좌·우 (모서리는 상·하에서 이미 채움)
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
    color_a: windows_sys::Win32::Foundation::COLORREF,
    color_b: windows_sys::Win32::Foundation::COLORREF,
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
            DeleteObject(brush);
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
            DeleteObject(brush);
            y += h;
            idx += 1;
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 리사이즈 히트 테스트
// ═══════════════════════════════════════════════════════════════════════════

fn hit_test_resize(wx: i32, wy: i32, fx: i32, fy: i32, sel_w: i32, sel_h: i32) -> Drag {
    let rx = wx - fx;
    let ry = wy - fy;
    let grip = sc(GRIP);
    if rx < -SHADOW || rx > sel_w + SHADOW || ry < -SHADOW || ry > sel_h + SHADOW {
        return Drag::None;
    }
    if rx > grip
        && rx < sel_w - grip
        && ry > grip
        && ry < sel_h - grip
    {
        return Drag::None;
    }
    let nl = rx <= grip;
    let nr = rx >= sel_w - grip;
    let nt = ry <= grip;
    let nb = ry >= sel_h - grip;
    match (nl, nr, nt, nb) {
        (true, _, true, _) => Drag::NW,
        (_, true, true, _) => Drag::NE,
        (true, _, _, true) => Drag::SW,
        (_, true, _, true) => Drag::SE,
        (_, _, true, _) => Drag::N,
        (_, _, _, true) => Drag::S,
        (true, _, _, _) => Drag::W,
        (_, true, _, _) => Drag::E,
        _ => Drag::None,
    }
}

fn cursor_for_drag(d: Drag) -> *const u16 {
    match d {
        Drag::N | Drag::S => IDC_SIZENS,
        Drag::E | Drag::W => IDC_SIZEWE,
        Drag::NW | Drag::SE => IDC_SIZENWSE,
        Drag::NE | Drag::SW => IDC_SIZENESW,
        Drag::None => IDC_ARROW,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 마우스 이벤트
// ═══════════════════════════════════════════════════════════════════════════

unsafe fn on_mouse_down(hwnd: HWND, lp: LPARAM) {
    // 녹화 중에는 영역 변경 불가
    if crate::recorder::is_recording() {
        return;
    }

    let cx = (lp as usize & 0xffff) as i16 as i32;
    let cy = ((lp as usize >> 16) & 0xffff) as i16 as i32;
    let (sel_w, sel_h, bar_pos) = read_sel_dims_pos();
    let geom = compute_geom(sel_w, sel_h, bar_pos);
    let mode = hit_test_resize(cx, cy, geom.fx, geom.fy, sel_w, sel_h);
    if mode == Drag::None {
        return;
    }

    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    let start_sel = STATE
        .lock()
        .expect("capture mutex poisoned")
        .as_ref()
        .map(|s| s.sel)
        .unwrap_or_else(|| unsafe { zeroed() });
    {
        let mut g = STATE.lock().expect("capture mutex poisoned");
        if let Some(s) = g.as_mut() {
            s.drag = Some(DragInfo {
                mode,
                start_cx: pt.x,
                start_cy: pt.y,
                start_sel,
            });
        }
    }
    SetCapture(hwnd);
}

unsafe fn on_mouse_move(hwnd: HWND) {
    let drag = {
        let g = STATE.lock().expect("capture mutex poisoned");
        let s = match g.as_ref() {
            Some(s) => s,
            None => return,
        };
        s.drag
            .as_ref()
            .map(|d| (d.mode, d.start_cx, d.start_cy, d.start_sel))
    };
    let (mode, start_cx, start_cy, start_sel) = match drag {
        Some(d) => d,
        None => return,
    };

    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    let dx = pt.x - start_cx;
    let dy = pt.y - start_cy;

    let mut sel = start_sel;
    match mode {
        Drag::N => sel.top = start_sel.top + dy,
        Drag::S => sel.bottom = start_sel.bottom + dy,
        Drag::W => sel.left = start_sel.left + dx,
        Drag::E => sel.right = start_sel.right + dx,
        Drag::NW => {
            sel.top = start_sel.top + dy;
            sel.left = start_sel.left + dx;
        }
        Drag::NE => {
            sel.top = start_sel.top + dy;
            sel.right = start_sel.right + dx;
        }
        Drag::SW => {
            sel.bottom = start_sel.bottom + dy;
            sel.left = start_sel.left + dx;
        }
        Drag::SE => {
            sel.bottom = start_sel.bottom + dy;
            sel.right = start_sel.right + dx;
        }
        Drag::None => return,
    }

    let aspect = STATE
        .lock()
        .expect("capture mutex poisoned")
        .as_ref()
        .and_then(|s| s.aspect);
    if let Some((rw, rh)) = aspect {
        apply_aspect_constraint(&mut sel, mode, start_sel, rw, rh);
    }

    if sel.right - sel.left < min_sel_w() {
        match mode {
            Drag::W | Drag::NW | Drag::SW => sel.left = sel.right - min_sel_w(),
            _ => sel.right = sel.left + min_sel_w(),
        }
    }
    if sel.bottom - sel.top < min_sel_h() {
        match mode {
            Drag::N | Drag::NW | Drag::NE => sel.top = sel.bottom - min_sel_h(),
            _ => sel.bottom = sel.top + min_sel_h(),
        }
    }

    if !normalize_sel(&mut sel) {
        return;
    }

    let new_w = sel.right - sel.left;
    let new_h = sel.bottom - sel.top;
    let bar_pos = {
        let mut g = STATE.lock().expect("capture mutex poisoned");
        if let Some(s) = g.as_mut() {
            s.sel = sel;
        }
        g.as_ref().map(|s| s.bar_pos).unwrap_or(BarPos::Top)
    };

    let geom = compute_geom(new_w, new_h, bar_pos);
    let (wx, wy) = win_pos(&sel, bar_pos);
    SetWindowPos(
        hwnd,
        null_mut(),
        wx,
        wy,
        geom.win_w,
        geom.win_h,
        SWP_NOZORDER | SWP_NOACTIVATE,
    );

    update_ctrl_positions(hwnd);
    apply_region(hwnd);
    InvalidateRect(hwnd, null(), 1);
    UpdateWindow(hwnd);
}

unsafe fn on_mouse_up() {
    ReleaseCapture();
    let mut g = STATE.lock().expect("capture mutex poisoned");
    if let Some(s) = g.as_mut() {
        s.drag = None;
    }
}

fn apply_aspect_constraint(sel: &mut RECT, mode: Drag, start_sel: RECT, rw: i32, rh: i32) {
    if rw <= 0 || rh <= 0 {
        return;
    }
    let w = sel.right - sel.left;
    let h = sel.bottom - sel.top;
    match mode {
        Drag::E | Drag::W => {
            let new_h = w * rh / rw;
            let cy = (start_sel.top + start_sel.bottom) / 2;
            sel.top = cy - new_h / 2;
            sel.bottom = cy + new_h / 2;
        }
        Drag::N | Drag::S => {
            let new_w = h * rw / rh;
            let cx = (start_sel.left + start_sel.right) / 2;
            sel.left = cx - new_w / 2;
            sel.right = cx + new_w / 2;
        }
        Drag::NW | Drag::NE | Drag::SW | Drag::SE => {
            let dw = (w - (start_sel.right - start_sel.left)).abs();
            let dh = (h - (start_sel.bottom - start_sel.top)).abs();
            if dw * rh >= dh * rw {
                let new_h = w * rh / rw;
                match mode {
                    Drag::NW | Drag::NE => sel.top = sel.bottom - new_h,
                    _ => sel.bottom = sel.top + new_h,
                }
            } else {
                let new_w = h * rw / rh;
                match mode {
                    Drag::NW | Drag::SW => sel.left = sel.right - new_w,
                    _ => sel.right = sel.left + new_w,
                }
            }
        }
        Drag::None => {}
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 버튼 / 콤보 이벤트
// ═══════════════════════════════════════════════════════════════════════════

unsafe fn on_toggle(hwnd: HWND) {
    if crate::recorder::is_recording() {
        on_stop(hwnd);
    } else {
        on_record(hwnd);
    }
}

unsafe fn on_record(hwnd: HWND) {
    // 설정 창에서 바꾼 저장 폴더 등이 프레임을 다시 열지 않아도 반영되도록 최신 값 사용
    let latest_config = crate::current_config();
    {
        let mut g = STATE.lock().expect("capture mutex poisoned");
        if let Some(s) = g.as_mut() {
            // 이전 녹화 마무리 전(인코더 종료 직후 틈, 저장 대화상자)에는 시작하지 않는다.
            if s.finish_pending {
                return;
            }
            normalize_sel(&mut s.sel);
            s.drag = None;
            s.cancelled = false;
            s.finish_pending = true;
            if let Some(config) = latest_config {
                s.config = config;
            }
        }
    }
    ReleaseCapture();

    let (sel, config) = {
        let g = STATE.lock().expect("capture mutex poisoned");
        let s = match g.as_ref() {
            Some(s) => s,
            None => return,
        };
        (s.sel, s.config.clone())
    };

    let mut st: windows_sys::Win32::Foundation::SYSTEMTIME = zeroed();
    GetLocalTime(&mut st);
    let filename = format!(
        "gif-{:04}{:02}{:02}-{:02}{:02}{:02}.gif",
        st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
    );
    let output_path = if config.ask_gif_save_path {
        std::env::temp_dir().join(format!("ccesdgr-{}", &filename))
    } else {
        let output_dir = std::path::PathBuf::from(&config.gif_output_path);
        let _ = std::fs::create_dir_all(&output_dir);
        output_dir.join(&filename)
    };
    {
        let mut g = STATE.lock().expect("capture mutex poisoned");
        if let Some(s) = g.as_mut() {
            s.suggested_gif_name = filename;
        }
    }
    // 첫 프레임에 핸들이 찍히지 않도록 화면에 반영된 뒤 녹화를 시작한다.
    apply_region(hwnd);
    UpdateWindow(hwnd);
    DwmFlush();

    let frame_w = sel.right - sel.left;
    let frame_h = sel.bottom - sel.top;
    let (cap_w, cap_h) = output_capture_size_from_frame(frame_w, frame_h, config.gif_even_size);

    let region = crate::recorder::CaptureRegion {
        x: sel.left + BORDER,
        y: sel.top + BORDER,
        width: cap_w,
        height: cap_h,
    };
    crate::recorder::start(
        region,
        output_path,
        config.show_gif_timestamp,
        config.gif_timestamp_font_size,
        config.gif_fps,
    );

    let (tb, preset, ratio) = {
        let g = STATE.lock().expect("capture mutex poisoned");
        let s = g.as_ref().expect("state missing");
        (s.toggle_btn as HWND, s.preset_ctrl as HWND, s.ratio_ctrl as HWND)
    };
    let stop_txt = win::to_wide("■");
    SetWindowTextW(tb, stop_txt.as_ptr());
    // 녹화 영역은 시작 시 고정되므로 크기 변경 컨트롤을 잠근다.
    EnableWindow(preset, 0);
    EnableWindow(ratio, 0);

    crate::tray::set_recording_tip(true);
    SetTimer(hwnd, TIMER_POLL, 200, None);
    InvalidateRect(hwnd, null(), 1);
    UpdateWindow(hwnd);
}

unsafe fn on_stop(hwnd: HWND) {
    crate::recorder::stop();
    {
        let mut g = STATE.lock().expect("capture mutex poisoned");
        if let Some(s) = g.as_mut() {
            s.is_stopping = true;
            s.cancelled = false;
        }
    }
    _ = hwnd;
}

unsafe fn on_cancel(hwnd: HWND) {
    let is_active = crate::recorder::is_recording() || {
        STATE
            .lock()
            .expect("capture mutex poisoned")
            .as_ref()
            .map(|s| s.is_stopping)
            .unwrap_or(false)
    };

    if is_active {
        let title = win::to_wide("GMTL Mouse Click");
        let message = win::to_wide("Discard this recording?");
        if MessageBoxW(
            hwnd,
            message.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_ICONQUESTION,
        ) != IDYES
        {
            return;
        }
        // 확인 창이 떠 있는 동안 poll_done이 이미 마무리했으면 폐기할 것이 없다.
        let still_pending = STATE
            .lock()
            .expect("capture mutex poisoned")
            .as_ref()
            .is_some_and(|s| s.finish_pending);
        if !still_pending {
            return;
        }
        crate::recorder::stop();
        let mut g = STATE.lock().expect("capture mutex poisoned");
        if let Some(s) = g.as_mut() {
            s.is_stopping = true;
            s.cancelled = true;
        }
    } else {
        DestroyWindow(hwnd);
    }
}

unsafe fn on_preset_changed(hwnd: HWND) {
    if crate::recorder::is_recording() {
        return;
    }
    let ctrl = {
        let g = STATE.lock().expect("capture mutex poisoned");
        g.as_ref().and_then(|s| {
            if s.preset_ctrl != 0 {
                Some(s.preset_ctrl as HWND)
            } else {
                None
            }
        })
    };
    let ctrl = match ctrl {
        Some(c) => c,
        None => return,
    };
    let idx = SendMessageW(ctrl, CB_GETCURSEL, 0, 0) as i64;
    let (pw, ph): (i32, i32) = match idx {
        1 => (640, 480),
        2 => (1280, 720),
        3 => (1920, 1080),
        4 => (1024, 768),
        5 => (800, 600),
        _ => return,
    };
    let (new_aspect, ratio_idx): (Option<(i32, i32)>, usize) = match idx {
        1 | 4 | 5 => (Some((4, 3)), 2),
        2 | 3 => (Some((16, 9)), 3),
        _ => (None, 0),
    };
    let (frame_w, frame_h) = frame_size_from_capture(pw, ph);
    let ratio_ctrl_h = {
        let mut g = STATE.lock().expect("capture mutex poisoned");
        let s = match g.as_mut() {
            Some(s) => s,
            None => return,
        };
        let cx = (s.sel.left + s.sel.right) / 2;
        let cy = (s.sel.top + s.sel.bottom) / 2;
        s.sel.left = cx - frame_w / 2;
        s.sel.right = s.sel.left + frame_w;
        s.sel.top = cy - frame_h / 2;
        s.sel.bottom = s.sel.top + frame_h;
        s.aspect = new_aspect;
        move_sel_into_screen(&mut s.sel);
        s.ratio_ctrl as HWND
    };
    if !ratio_ctrl_h.is_null() {
        SendMessageW(ratio_ctrl_h, CB_SETCURSEL, ratio_idx, 0);
    }
    refresh_capture_window(hwnd);
    SendMessageW(ctrl, CB_SETCURSEL, 0, 0);
}

unsafe fn on_ratio_changed(hwnd: HWND) {
    if crate::recorder::is_recording() {
        return;
    }
    let ctrl = {
        let g = STATE.lock().expect("capture mutex poisoned");
        g.as_ref().and_then(|s| {
            if s.ratio_ctrl != 0 {
                Some(s.ratio_ctrl as HWND)
            } else {
                None
            }
        })
    };
    let ctrl = match ctrl {
        Some(c) => c,
        None => return,
    };
    let idx = SendMessageW(ctrl, CB_GETCURSEL, 0, 0) as i64;
    let aspect: Option<(i32, i32)> = match idx {
        1 => Some((1, 1)),
        2 => Some((4, 3)),
        3 => Some((16, 9)),
        4 => Some((9, 16)),
        _ => None,
    };
    {
        let mut g = STATE.lock().expect("capture mutex poisoned");
        let s = match g.as_mut() {
            Some(s) => s,
            None => return,
        };
        s.aspect = aspect;

        if let Some((rw, rh)) = aspect {
            let frame_h = (s.sel.bottom - s.sel.top).max(min_sel_h());
            let (_, cap_h) = capture_size_from_frame(s.sel.right - s.sel.left, frame_h);
            let min_cap_w = (min_sel_w() - BORDER * 2).max(1);
            let new_cap_w = (cap_h * rw / rh).max(min_cap_w);
            let (new_w, _) = frame_size_from_capture(new_cap_w, cap_h);
            let cx = (s.sel.left + s.sel.right) / 2;
            s.sel.left = cx - new_w / 2;
            s.sel.right = s.sel.left + new_w;
        }
        move_sel_into_screen(&mut s.sel);
    }
    refresh_capture_window(hwnd);
}

unsafe fn on_bar_pos_changed(hwnd: HWND) {
    let ctrl = {
        let g = STATE.lock().expect("capture mutex poisoned");
        g.as_ref().and_then(|s| {
            if s.bar_pos_ctrl != 0 {
                Some(s.bar_pos_ctrl as HWND)
            } else {
                None
            }
        })
    };
    let ctrl = match ctrl {
        Some(c) => c,
        None => return,
    };
    let idx = SendMessageW(ctrl, CB_GETCURSEL, 0, 0) as i64;
    let new_pos = match idx {
        0 => BarPos::Top,
        _ => BarPos::Bottom,
    };
    {
        let mut g = STATE.lock().expect("capture mutex poisoned");
        if let Some(s) = g.as_mut() {
            s.bar_pos = new_pos;
        }
    }
    reposition_window(hwnd);
}

unsafe fn reposition_window(hwnd: HWND) {
    auto_adjust_bar_pos();
    let (sel, bar_pos) = {
        let g = STATE.lock().expect("capture mutex poisoned");
        let s = match g.as_ref() {
            Some(s) => s,
            None => return,
        };
        (s.sel, s.bar_pos)
    };
    let sel_w = sel.right - sel.left;
    let sel_h = sel.bottom - sel.top;
    let geom = compute_geom(sel_w, sel_h, bar_pos);
    let (wx, wy) = win_pos(&sel, bar_pos);
    SetWindowPos(
        hwnd,
        null_mut(),
        wx,
        wy,
        geom.win_w,
        geom.win_h,
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
    rebuild_controls(hwnd);
    apply_region(hwnd);
    InvalidateRect(hwnd, null(), 1);
    UpdateWindow(hwnd);
}

unsafe fn refresh_capture_window(hwnd: HWND) {
    auto_adjust_bar_pos();
    let (sel, bar_pos) = {
        let g = STATE.lock().expect("capture mutex poisoned");
        let s = match g.as_ref() {
            Some(s) => s,
            None => return,
        };
        (s.sel, s.bar_pos)
    };
    let sel_w = sel.right - sel.left;
    let sel_h = sel.bottom - sel.top;
    let geom = compute_geom(sel_w, sel_h, bar_pos);
    let (wx, wy) = win_pos(&sel, bar_pos);
    SetWindowPos(
        hwnd,
        null_mut(),
        wx,
        wy,
        geom.win_w,
        geom.win_h,
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
    update_ctrl_positions(hwnd);
    apply_region(hwnd);
    InvalidateRect(hwnd, null(), 1);
    UpdateWindow(hwnd);
}

fn auto_adjust_bar_pos() -> bool {
    let mut g = STATE.lock().expect("capture mutex poisoned");
    let Some(s) = g.as_mut() else { return false };

    let screen_top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let screen_bottom = screen_top + unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    let sel_h = s.sel.bottom - s.sel.top;
    let old_pos = s.bar_pos;
    let top_over = s.sel.top - bar_h() < screen_top;
    let bottom_over = s.sel.top + sel_h + bar_h() > screen_bottom;

    // 위아래 어느 쪽에도 툴바가 안 들어가면 프레임을 올려 아래쪽에 자리를 만든다.
    // (높이는 normalize_sel의 max_sel_height로 제한되어 있어 항상 들어간다.)
    if top_over && bottom_over {
        let moved = s.sel.top != screen_top;
        s.sel.top = screen_top;
        s.sel.bottom = screen_top + sel_h;
        s.bar_pos = BarPos::Bottom;
        return moved || old_pos != BarPos::Bottom;
    }

    s.bar_pos = match s.bar_pos {
        BarPos::Top if top_over && !bottom_over => BarPos::Bottom,
        BarPos::Bottom if bottom_over && !top_over => BarPos::Top,
        pos => pos,
    };

    s.bar_pos != old_pos
}

// ═══════════════════════════════════════════════════════════════════════════
// 녹화 완료 폴링
// ═══════════════════════════════════════════════════════════════════════════

unsafe fn poll_done(hwnd: HWND) {
    if crate::recorder::is_recording() {
        // 녹화 중: 툴바의 경과 시간만 갱신
        invalidate_bar_text(hwnd);
        return;
    }
    SAVING.store(true, Ordering::Relaxed);
    finish_recording(hwnd);
    // 저장 대화상자가 끝난 뒤에야 새 녹화를 허용한다.
    let window_alive = match STATE.lock().expect("capture mutex poisoned").as_mut() {
        Some(s) => {
            s.finish_pending = false;
            true
        }
        None => false,
    };
    if window_alive {
        // 숨겼던 리사이즈 핸들 복원
        apply_region(hwnd);
        InvalidateRect(hwnd, null(), 1);
    }
    SAVING.store(false, Ordering::Relaxed);
}

unsafe fn finish_recording(hwnd: HWND) {
    let cancelled = STATE
        .lock()
        .expect("capture mutex poisoned")
        .as_ref()
        .map(|s| s.cancelled)
        .unwrap_or(false);

    // 중지 요청(■/트레이/핫키) 또는 녹화 스레드 자체 종료(오류) 모두 여기서 마무리
    KillTimer(hwnd, TIMER_POLL);
    crate::tray::set_recording_tip(false);

    let (tb, preset, ratio) = {
        let g = STATE.lock().expect("capture mutex poisoned");
        let s = match g.as_ref() {
            Some(s) => s,
            None => return,
        };
        (s.toggle_btn as HWND, s.preset_ctrl as HWND, s.ratio_ctrl as HWND)
    };
    let rec_txt = win::to_wide("●");
    SetWindowTextW(tb, rec_txt.as_ptr());
    EnableWindow(preset, 1);
    EnableWindow(ratio, 1);

    {
        let mut g = STATE.lock().expect("capture mutex poisoned");
        if let Some(s) = g.as_mut() {
            s.is_stopping = false;
        }
    }

    if cancelled {
        let output_path = crate::recorder::last_output_path();
        let _ = std::fs::remove_file(&output_path);
        DestroyWindow(hwnd);
        return;
    }

    if let Some(message) = crate::recorder::take_last_error() {
        let output_path = crate::recorder::last_output_path();
        let _ = std::fs::remove_file(&output_path);
        InvalidateRect(hwnd, null(), 1);
        show_error(hwnd, &format!("GIF recording failed.\n{message}"));
        return;
    }

    if crate::recorder::last_frame_count() == 0 {
        let output_path = crate::recorder::last_output_path();
        let _ = std::fs::remove_file(&output_path);
        InvalidateRect(hwnd, null(), 1);
        crate::tray::notify("Recording stopped — no frames were captured.");
        return;
    }

    InvalidateRect(hwnd, null(), 1);
    UpdateWindow(hwnd);

    let (open_folder, ask_save, initial_dir, suggested_name) = {
        let g = STATE.lock().expect("capture mutex poisoned");
        let s = g.as_ref().expect("state missing");
        (
            s.config.open_folder_after_save,
            s.config.ask_gif_save_path,
            std::path::PathBuf::from(&s.config.gif_output_path),
            s.suggested_gif_name.clone(),
        )
    };

    let mut output_path = crate::recorder::last_output_path();
    if ask_save {
        let default_name = if suggested_name.is_empty() {
            "recording.gif".to_string()
        } else {
            suggested_name
        };
        let dest = loop {
            match win::save_gif_dialog(hwnd, &initial_dir, &default_name) {
                Ok(Some(dest)) => break dest,
                Ok(None) => {
                    // Cancel: 실수로 녹화를 잃지 않게 확인 후 삭제, 아니오면 대화상자로 복귀
                    let title = win::to_wide("GMTL Mouse Click");
                    let message = win::to_wide("Discard this recording without saving?");
                    if MessageBoxW(
                        hwnd,
                        message.as_ptr(),
                        title.as_ptr(),
                        MB_YESNO | MB_ICONQUESTION,
                    ) == IDYES
                    {
                        let _ = std::fs::remove_file(&output_path);
                        return;
                    }
                }
                Err(code) => {
                    // 대화상자 오류는 취소가 아니다 — 녹화를 지우지 않는다.
                    show_error(
                        hwnd,
                        &format!(
                            "The save dialog failed (error 0x{code:X}).\nThe recording is kept at {}",
                            output_path.display()
                        ),
                    );
                    return;
                }
            }
        };
        if dest != output_path {
            if let Err(error) = move_file(&output_path, &dest) {
                // 임시 파일은 남겨 두어 녹화를 잃지 않게 한다.
                show_error(
                    hwnd,
                    &format!(
                        "Failed to save the GIF to {}: {error}\nThe recording is kept at {}",
                        dest.display(),
                        output_path.display()
                    ),
                );
                return;
            }
            crate::recorder::set_last_output_path(dest.clone());
            output_path = dest;
        }
    }

    let file_name = output_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    crate::tray::notify(&format!("GIF saved: {file_name}"));

    if open_folder {
        if let Some(folder) = output_path.parent() {
            let path_w = win::to_wide(&folder.to_string_lossy());
            let op_w = win::to_wide("explore");
            ShellExecuteW(
                null_mut(),
                op_w.as_ptr(),
                path_w.as_ptr(),
                null_mut(),
                null_mut(),
                SW_SHOWDEFAULT,
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 유틸리티
// ═══════════════════════════════════════════════════════════════════════════

/// 바 왼쪽의 크기/경과 시간 텍스트 영역만 다시 그린다 (컨트롤 깜빡임 방지).
unsafe fn invalidate_bar_text(hwnd: HWND) {
    let (sel_w, sel_h, bar_pos) = read_sel_dims_pos();
    let geom = compute_geom(sel_w, sel_h, bar_pos);
    let rect = RECT {
        left: geom.bar_l,
        top: geom.bar_t,
        right: (geom.bar_l + sc(BAR_TEXT_X + BAR_TEXT_W)).min(geom.bar_r),
        bottom: geom.bar_b,
    };
    InvalidateRect(hwnd, &rect, 0);
}

fn show_error(hwnd: HWND, message: &str) {
    let title = win::to_wide("GMTL Mouse Click");
    let message = win::to_wide(message);
    unsafe {
        MessageBoxW(hwnd, message.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR);
    }
}

fn read_sel_dims_pos() -> (i32, i32, BarPos) {
    let g = STATE.lock().expect("capture mutex poisoned");
    match g.as_ref() {
        Some(s) => (
            s.sel.right - s.sel.left,
            s.sel.bottom - s.sel.top,
            s.bar_pos,
        ),
        None => (min_sel_w(), min_sel_h(), BarPos::Top),
    }
}

fn capture_size_from_frame(frame_w: i32, frame_h: i32) -> (i32, i32) {
    ((frame_w - BORDER * 2).max(1), (frame_h - BORDER * 2).max(1))
}

fn output_capture_size_from_frame(frame_w: i32, frame_h: i32, even_size: bool) -> (i32, i32) {
    let (width, height) = capture_size_from_frame(frame_w, frame_h);
    if even_size {
        (make_even(width), make_even(height))
    } else {
        (width, height)
    }
}

fn make_even(value: i32) -> i32 {
    if value > 1 && value % 2 != 0 {
        value - 1
    } else {
        value
    }
}

fn frame_size_from_capture(cap_w: i32, cap_h: i32) -> (i32, i32) {
    (
        (cap_w + BORDER * 2).max(min_sel_w()),
        (cap_h + BORDER * 2).max(min_sel_h()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use windows_sys::Win32::UI::WindowsAndMessaging::GetDlgItem;

    /// DPI 전역값을 읽거나 바꾸는 테스트끼리 병렬 실행되지 않게 한다.
    static DPI_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn dpi_lock() -> std::sync::MutexGuard<'static, ()> {
        DPI_TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// 실제 창에 DPI 변경을 보낸다 (데스크톱 세션 필요): `cargo test -- --ignored`
    /// 다른 프로세스에서는 Windows가 WM_DPICHANGED 전송을 막아 프로세스 안에서 시험한다.
    #[test]
    #[ignore]
    fn dpi_change_rescales_toolbar_and_keeps_frame() {
        let _guard = dpi_lock();
        let rect_of = |hwnd: HWND| unsafe {
            let mut rect: RECT = zeroed();
            GetWindowRect(hwnd, &mut rect);
            rect
        };
        show(null_mut(), AppConfig::default());
        let hwnd = CAPTURE_HWND.load(Ordering::Relaxed) as HWND;
        assert!(!hwnd.is_null());
        unsafe {
            let suggested: RECT = zeroed();
            let send_dpi = |dpi: usize| {
                SendMessageW(
                    hwnd,
                    WM_DPICHANGED,
                    dpi | (dpi << 16),
                    &suggested as *const RECT as LPARAM,
                );
            };
            send_dpi(96);
            let before = rect_of(hwnd);
            let sel_before = STATE.lock().unwrap().as_ref().unwrap().sel;

            send_dpi(192);
            let button = rect_of(GetDlgItem(hwnd, ID_TOGGLE));
            assert_eq!(button.right - button.left, BTN_W * 2);
            assert_eq!(button.bottom - button.top, BTN_H * 2);
            let doubled = rect_of(hwnd);
            let sel = STATE.lock().unwrap().as_ref().unwrap().sel;
            assert_eq!(sel.right - sel.left, sel_before.right - sel_before.left);
            assert_eq!(sel.bottom - sel.top, sel_before.bottom - sel_before.top);
            assert_eq!(doubled.bottom, before.bottom, "frame must not move");
            assert_eq!(doubled.bottom - doubled.top, before.bottom - before.top + BAR_H);

            send_dpi(96);
            let button = rect_of(GetDlgItem(hwnd, ID_TOGGLE));
            assert_eq!(button.right - button.left, BTN_W);
            let restored = rect_of(hwnd);
            assert_eq!(restored.top, before.top);
            assert_eq!(restored.bottom, before.bottom);
        }
        close();
        DPI.store(96, Ordering::Relaxed);
    }

    #[test]
    fn toolbar_scales_with_dpi() {
        let normal = toolbar_layout(800, 0, 96);
        let double = toolbar_layout(1600, 0, 192);
        assert_eq!((normal.btn_w, normal.combo_w), (BTN_W, COMBO_W));
        assert_eq!((double.btn_w, double.combo_w), (BTN_W * 2, COMBO_W * 2));
        assert_eq!(double.preset_x, normal.preset_x * 2);
    }

    #[test]
    fn narrow_toolbar_hides_combos_instead_of_covering_text() {
        let wide = toolbar_layout(800, 0, 96);
        assert!(wide.show_preset && wide.show_ratio && wide.show_bar_pos);
        let narrow = toolbar_layout(MIN_SEL_W, 0, 96);
        assert!(!narrow.show_preset);
        // 보이는 컨트롤은 문구 영역을 침범하지 않는다.
        let text_end = BAR_TEXT_X + BAR_TEXT_W;
        for (show, x) in [
            (narrow.show_preset, narrow.preset_x),
            (narrow.show_ratio, narrow.ratio_x),
            (narrow.show_bar_pos, narrow.bar_pos_x),
        ] {
            assert!(!show || x >= text_end);
        }
    }

    #[test]
    fn grips_stay_inside_the_frame() {
        let rects = grip_rects(0, 32, 640, 480, 12);
        for grip in rects {
            assert!(grip.left >= 0 && grip.right <= 640);
            assert!(grip.top >= 32 && grip.bottom <= 32 + 480);
        }
        assert_eq!((rects[0].left, rects[0].top), (0, 32));
        assert_eq!((rects[7].right, rects[7].bottom), (640, 512));
    }

    #[test]
    fn grip_hit_test_matches_resize_direction() {
        let _guard = dpi_lock();
        // 96 DPI 기준 (fx=0, fy=0, 640x480)
        assert!(hit_test_resize(3, 3, 0, 0, 640, 480) == Drag::NW);
        assert!(hit_test_resize(636, 476, 0, 0, 640, 480) == Drag::SE);
        assert!(hit_test_resize(320, 3, 0, 0, 640, 480) == Drag::N);
        assert!(hit_test_resize(3, 240, 0, 0, 640, 480) == Drag::W);
        assert!(hit_test_resize(320, 240, 0, 0, 640, 480) == Drag::None);
    }

    #[test]
    fn frame_height_leaves_room_for_toolbar() {
        let _guard = dpi_lock();
        assert_eq!(max_sel_height(1080), 1080 - bar_h());
        assert_eq!(max_sel_height(50), min_sel_h());
    }

    #[test]
    fn capture_area_excludes_border() {
        assert_eq!(capture_size_from_frame(642, 482), (640, 480));
    }

    #[test]
    fn even_size_rounds_odd_dimensions_down() {
        assert_eq!(output_capture_size_from_frame(643, 483, true), (640, 480));
        assert_eq!(output_capture_size_from_frame(643, 483, false), (641, 481));
    }

    #[test]
    fn frame_size_adds_border_and_respects_minimum() {
        let _guard = dpi_lock();
        assert_eq!(frame_size_from_capture(640, 480), (642, 482));
        assert_eq!(frame_size_from_capture(10, 10), (min_sel_w(), min_sel_h()));
    }
}
