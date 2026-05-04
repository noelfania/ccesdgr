use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CombineRgn, CreateRectRgn, CreateSolidBrush, DeleteObject, EndPaint,
    FillRect, GetStockObject, InvalidateRect,
    RGN_DIFF, RGN_OR, SelectObject, SetBkMode, SetTextColor,
    SetWindowRgn, TextOutW, UpdateWindow, DEFAULT_GUI_FONT,
    PAINTSTRUCT,
};
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, ReleaseCapture, SetCapture, SetFocus,
};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BS_PUSHBUTTON, CreateWindowExW, DefWindowProcW, DestroyWindow,
    GetCursorPos, GetSystemMetrics, GetWindowRect,
    HMENU, HTCAPTION, HTCLIENT,
    IDC_ARROW, IDC_SIZENS, IDC_SIZEWE, IDC_SIZENWSE, IDC_SIZENESW,
    KillTimer, LoadCursorW, RegisterClassExW, SendMessageW, SetCursor,
    SetTimer, SetWindowPos, SetWindowTextW, ShowWindow,
    SM_CXSCREEN, SM_CYSCREEN, SM_CYVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, SW_SHOW, SW_SHOWDEFAULT,
    SWP_NOACTIVATE, SWP_NOZORDER,
    WM_COMMAND, WM_CREATE, WM_DESTROY, WM_ERASEBKGND,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOVE, WM_NCHITTEST,
    WM_PAINT, WM_SETCURSOR, WM_SETFONT, WM_TIMER, WNDCLASSEXW,
    WS_CHILD, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_TABSTOP,
    WS_VISIBLE, WINDOW_STYLE,
};

use crate::config::AppConfig;
use crate::win;

// ── 콤보박스 상수 (windows-sys에서 직접 임포트 불가한 항목) ───────────────
const CBS_DROPDOWNLIST: WINDOW_STYLE = 3;
const CB_ADDSTRING:     u32 = 0x0143;
const CB_SETCURSEL:     u32 = 0x014E;
const CB_GETCURSEL:     u32 = 0x0147;
const CBN_SELCHANGE:    i32 = 1;

// ── 레이아웃 상수 ─────────────────────────────────────────────────────────
const CLASS_NAME:  &str = "GMTLCaptureUI";

const BAR_H:       i32 = 32;  // 바 높이
const BORDER:      i32 = 4;   // 캡처 프레임 테두리 두께
const SHADOW:      i32 = 0;   // 프레임 외부 그림자 없음
const HANDLE:      i32 = 10;  // 리사이즈 핸들 감지 반경
const MIN_SEL_W:   i32 = 320; // 최소 캡처 너비
const MIN_SEL_H:   i32 = 100; // 최소 캡처 높이
const TIMER_POLL:  usize = 1;

// ── 컨트롤 ID ────────────────────────────────────────────────────────────
const ID_TOGGLE:       i32 = 3001;
const ID_CANCEL:       i32 = 3003;
const ID_BAR_POS:      i32 = 3004;
const ID_PRESET_COMBO: i32 = 3005;
const ID_RATIO_COMBO:  i32 = 3006;

// ── 버튼/콤보 치수 ─────────────────────────────────────────────────────────
const BTN_W: i32 = 26;
const BTN_H: i32 = 26;

// ── 타입 정의 ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum BarPos { Top, Bottom }

#[derive(Clone, Copy, PartialEq)]
enum Drag { None, N, S, E, W, NW, NE, SW, SE }

struct State {
    sel:          RECT,
    drag:         Option<DragInfo>,
    toggle_btn:   isize,
    cancel_btn:   isize,
    bar_pos_ctrl: isize,
    preset_ctrl:  isize,
    ratio_ctrl:   isize,
    is_stopping:  bool,
    cancelled:    bool,
    config:       AppConfig,
    aspect:       Option<(i32, i32)>,
    bar_pos:      BarPos,
}

struct DragInfo {
    mode:      Drag,
    start_cx:  i32,
    start_cy:  i32,
    start_sel: RECT,
}

/// 창 내 좌표 기준으로 캡처 프레임 좌상단 위치
struct WinGeom {
    win_w:    i32,
    win_h:    i32,
    // 바 영역 (창 좌표)
    bar_l: i32, bar_t: i32, bar_r: i32, bar_b: i32,
    // 캡처 프레임 시작점 (창 좌표)
    fx: i32, fy: i32,
}

fn compute_geom(sel_w: i32, sel_h: i32, bar_pos: BarPos) -> WinGeom {
    match bar_pos {
        BarPos::Top => WinGeom {
            win_w: sel_w + 2 * SHADOW,
            win_h: BAR_H + sel_h + SHADOW,
            bar_l: 0,    bar_t: 0,
            bar_r: sel_w + 2 * SHADOW, bar_b: BAR_H,
            fx: SHADOW, fy: BAR_H,
        },
        BarPos::Bottom => WinGeom {
            win_w: sel_w + 2 * SHADOW,
            win_h: SHADOW + sel_h + BAR_H,
            bar_l: 0,    bar_t: SHADOW + sel_h,
            bar_r: sel_w + 2 * SHADOW, bar_b: SHADOW + sel_h + BAR_H,
            fx: SHADOW, fy: SHADOW,
        },
    }
}

fn win_pos(sel: &RECT, bar_pos: BarPos) -> (i32, i32) {
    match bar_pos {
        BarPos::Top    => (sel.left - SHADOW, sel.top - BAR_H),
        BarPos::Bottom => (sel.left - SHADOW, sel.top - SHADOW),
    }
}

// ── 정적 상태 ────────────────────────────────────────────────────────────
static CAPTURE_HWND: AtomicIsize = AtomicIsize::new(0);
static STATE: Mutex<Option<State>> = Mutex::new(None);

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
    unsafe { RegisterClassExW(&class); }

    let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let sh = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let sel_w = 640_i32.min(sw - 100).max(MIN_SEL_W);
    let sel_h = 480_i32.min(sh - 200).max(MIN_SEL_H);
    let sel = RECT {
        left:   (sw - sel_w) / 2,
        top:    (sh - sel_h) / 2,
        right:  (sw + sel_w) / 2,
        bottom: (sh + sel_h) / 2,
    };
    let bar_pos = BarPos::Top;
    let geom = compute_geom(sel_w, sel_h, bar_pos);
    let (wx, wy) = win_pos(&sel, bar_pos);

    *STATE.lock().expect("capture mutex poisoned") = Some(State {
        sel,
        drag: None,
        toggle_btn: 0, cancel_btn: 0,
        bar_pos_ctrl: 0, preset_ctrl: 0, ratio_ctrl: 0,
        is_stopping: false, cancelled: false,
        config,
        aspect: None,
        bar_pos,
    });

    let title = win::to_wide("GIF 녹화");
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name.as_ptr(), title.as_ptr(),
            WS_POPUP,
            wx, wy, geom.win_w, geom.win_h,
            owner, null_mut(), instance, null_mut(),
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

pub fn close() {
    let hwnd = CAPTURE_HWND.load(Ordering::Relaxed);
    if hwnd != 0 {
        unsafe { DestroyWindow(hwnd as HWND); }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 메인 창 프로시저
// ═══════════════════════════════════════════════════════════════════════════

unsafe extern "system" fn capture_proc(
    hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            create_controls(hwnd);
            apply_region(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_NCHITTEST => {
            let mut wr: RECT = zeroed();
            GetWindowRect(hwnd, &mut wr);
            let my = ((lp as usize >> 16) & 0xffff) as i16 as i32;
            let mx = (lp as usize & 0xffff) as i16 as i32;
            let wy = my - wr.top;
            let wx = mx - wr.left;
            let (sel_w, sel_h, bar_pos) = read_sel_dims_pos();
            let geom = compute_geom(sel_w, sel_h, bar_pos);
            if wx >= geom.bar_l && wx < geom.bar_r
                && wy >= geom.bar_t && wy < geom.bar_b
            {
                if crate::recorder::is_recording() {
                    return HTCLIENT as LRESULT;
                }
                return HTCAPTION as LRESULT;
            }
            HTCLIENT as LRESULT
        }
        WM_SETCURSOR => {
            let ht = win::low_word(lp as usize) as i32;
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
            if let Some(s) = g.as_mut() {
                let w = s.sel.right - s.sel.left;
                let h = s.sel.bottom - s.sel.top;
                let geom = compute_geom(w, h, s.bar_pos);
                s.sel.left   = nx + geom.fx;
                s.sel.top    = ny + geom.fy;
                s.sel.right  = s.sel.left + w;
                s.sel.bottom = s.sel.top  + h;
            }
            drop(g);
            if auto_adjust_bar_pos() {
                reposition_window(hwnd);
            }
            0
        }
        WM_PAINT => { paint(hwnd); 0 }
        WM_LBUTTONDOWN => { on_mouse_down(hwnd, lp); 0 }
        WM_MOUSEMOVE   => { on_mouse_move(hwnd); 0 }
        WM_LBUTTONUP   => { on_mouse_up(); 0 }
        WM_COMMAND => {
            let id   = win::low_word(wp as usize) as i32;
            let note = ((wp as usize >> 16) & 0xffff) as i32;
            match id {
                ID_TOGGLE => { if note == 0 { on_toggle(hwnd); } 0 }
                ID_CANCEL => { if note == 0 { on_cancel(hwnd); } 0 }
                ID_BAR_POS      if note == CBN_SELCHANGE => { on_bar_pos_changed(hwnd); 0 }
                ID_PRESET_COMBO if note == CBN_SELCHANGE => { on_preset_changed(hwnd); 0 }
                ID_RATIO_COMBO  if note == CBN_SELCHANGE => { on_ratio_changed(hwnd); 0 }
                _ => DefWindowProcW(hwnd, msg, wp, lp),
            }
        }
        WM_TIMER => { if wp == TIMER_POLL { poll_done(hwnd); } 0 }
        WM_DESTROY => {
            KillTimer(hwnd, TIMER_POLL);
            CAPTURE_HWND.store(0, Ordering::Relaxed);
            *STATE.lock().expect("capture mutex poisoned") = None;
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
            0, cls.as_ptr(), txt.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON as WINDOW_STYLE,
            x, y, w, h,
            hwnd, id as isize as HMENU, instance, null_mut(),
        );
        if !btn.is_null() {
            let font = GetStockObject(DEFAULT_GUI_FONT);
            SendMessageW(btn, WM_SETFONT, font as WPARAM, 1);
        }
        btn as isize
    };

    let make_combo = |id: i32, x: i32, y: i32, w: i32, h: i32| -> isize {
        let cls = win::to_wide("COMBOBOX");
        let combo = CreateWindowExW(
            0, cls.as_ptr(), null(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | CBS_DROPDOWNLIST,
            x, y, w, h + 200,
            hwnd, id as isize as HMENU, instance, null_mut(),
        );
        if !combo.is_null() {
            let font = GetStockObject(DEFAULT_GUI_FONT);
            SendMessageW(combo, WM_SETFONT, font as WPARAM, 1);
        }
        combo as isize
    };

    let add_str = |combo: isize, s: &str| {
        let ws = win::to_wide(s);
        SendMessageW(combo as HWND, CB_ADDSTRING, 0, ws.as_ptr() as LPARAM);
    };

    let (toggle_btn, cancel_btn, bar_pos_ctrl, preset_ctrl, ratio_ctrl);

    let by = geom.bar_t;
    let bty = by + (BAR_H - BTN_H) / 2;
    let bcy = by + (BAR_H - 22) / 2;
    let win_w = geom.win_w;

    let cx_cancel  = win_w - 4 - BTN_W;
    let cx_toggle  = cx_cancel - 4 - BTN_W;
    let cx_barpos  = cx_toggle - 4 - 65;
    let cx_ratio   = cx_barpos - 4 - 65;
    let cx_preset  = cx_ratio  - 4 - 85;

    cancel_btn   = make_btn(ID_CANCEL, "✕",  cx_cancel,  bty, BTN_W, BTN_H);
    toggle_btn   = make_btn(ID_TOGGLE, "●",  cx_toggle,  bty, BTN_W, BTN_H);

    bar_pos_ctrl = make_combo(ID_BAR_POS,      cx_barpos, bcy, 65, 22);
    ratio_ctrl   = make_combo(ID_RATIO_COMBO,  cx_ratio,  bcy, 65, 22);
    preset_ctrl  = make_combo(ID_PRESET_COMBO, cx_preset, bcy, 85, 22);

    add_str(bar_pos_ctrl, "위");
    add_str(bar_pos_ctrl, "아래");
    let bp_idx = match bar_pos { BarPos::Top => 0, BarPos::Bottom => 1 };
    SendMessageW(bar_pos_ctrl as HWND, CB_SETCURSEL, bp_idx, 0);

    for s in ["자유","1:1","4:3","16:9","9:16"] { add_str(ratio_ctrl, s); }
    let ratio_idx = match aspect {
        None          => 0, Some((1,1)) => 1, Some((4,3)) => 2,
        Some((16,9))  => 3, Some((9,16)) => 4, _          => 0,
    };
    SendMessageW(ratio_ctrl as HWND, CB_SETCURSEL, ratio_idx, 0);

    for s in ["크기","640x480","1280x720","1920x1080","1024x768","800x600"] {
        add_str(preset_ctrl, s);
    }
    SendMessageW(preset_ctrl as HWND, CB_SETCURSEL, 0, 0);

    if is_recording {
        let tw = win::to_wide("■");
        SetWindowTextW(toggle_btn as HWND, tw.as_ptr());
        EnableWindow(cancel_btn as HWND, 0);
    }

    {
        let mut g = STATE.lock().expect("capture mutex poisoned");
        if let Some(s) = g.as_mut() {
            s.toggle_btn   = toggle_btn;
            s.cancel_btn   = cancel_btn;
            s.bar_pos_ctrl = bar_pos_ctrl;
            s.preset_ctrl  = preset_ctrl;
            s.ratio_ctrl   = ratio_ctrl;
        }
    }
}

unsafe fn destroy_controls() {
    let (t, c, b, p, r) = {
        let g = STATE.lock().expect("capture mutex poisoned");
        let s = match g.as_ref() { Some(s) => s, None => return };
        (s.toggle_btn as HWND, s.cancel_btn as HWND, s.bar_pos_ctrl as HWND,
         s.preset_ctrl as HWND, s.ratio_ctrl as HWND)
    };
    for h in [t, c, b, p, r] {
        if !h.is_null() { DestroyWindow(h); }
    }
    let mut g = STATE.lock().expect("capture mutex poisoned");
    if let Some(s) = g.as_mut() {
        s.toggle_btn = 0; s.cancel_btn = 0;
        s.bar_pos_ctrl = 0; s.preset_ctrl = 0; s.ratio_ctrl = 0;
    }
}

unsafe fn rebuild_controls(hwnd: HWND) {
    destroy_controls();
    create_controls(hwnd);
}

unsafe fn update_ctrl_positions(hwnd: HWND) {
    let (bar_pos, sel_w, sel_h, t, c, b, p, r) = {
        let g = STATE.lock().expect("capture mutex poisoned");
        let s = match g.as_ref() { Some(s) => s, None => return };
        let w = s.sel.right - s.sel.left;
        let h = s.sel.bottom - s.sel.top;
        (s.bar_pos, w, h,
         s.toggle_btn as HWND, s.cancel_btn as HWND, s.bar_pos_ctrl as HWND,
         s.preset_ctrl as HWND, s.ratio_ctrl as HWND)
    };

    let geom = compute_geom(sel_w, sel_h, bar_pos);
    let by   = geom.bar_t;
    let bty  = by + (BAR_H - BTN_H) / 2;
    let bcy  = by + (BAR_H - 22) / 2;
    let ww   = geom.win_w;

    let cx_cancel = ww - 4 - BTN_W;
    let cx_toggle = cx_cancel - 4 - BTN_W;
    let cx_barpos = cx_toggle - 4 - 65;
    let cx_ratio  = cx_barpos - 4 - 65;
    let cx_preset = cx_ratio  - 4 - 85;

    let fl = SWP_NOZORDER | SWP_NOACTIVATE;
    if !b.is_null() {
        let bp_idx = match bar_pos { BarPos::Top => 0, BarPos::Bottom => 1 };
        SendMessageW(b, CB_SETCURSEL, bp_idx, 0);
    }
    if !c.is_null() { SetWindowPos(c, null_mut(), cx_cancel, bty, BTN_W, BTN_H, fl); }
    if !t.is_null() { SetWindowPos(t, null_mut(), cx_toggle, bty, BTN_W, BTN_H, fl); }
    if !b.is_null() { SetWindowPos(b, null_mut(), cx_barpos, bcy, 65, 22, fl); }
    if !r.is_null() { SetWindowPos(r, null_mut(), cx_ratio,  bcy, 65, 22, fl); }
    if !p.is_null() { SetWindowPos(p, null_mut(), cx_preset, bcy, 85, 22, fl); }
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
        BarPos::Top    => (0, geom.bar_b, geom.win_w, geom.win_h),
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
    let geom     = compute_geom(sel_w, sel_h, bar_pos);
    let recording = crate::recorder::is_recording();

    // ── 바 배경 ─────────────────────────────────────────────────────────
    let bar_brush = CreateSolidBrush(win::colorref(40, 40, 40));
    FillRect(hdc, &RECT { left: geom.bar_l, top: geom.bar_t,
                          right: geom.bar_r, bottom: geom.bar_b }, bar_brush);
    DeleteObject(bar_brush);

    // ── 크기 텍스트 ─────────────────────────────────────────────────────
    SetBkMode(hdc, 1);
    SetTextColor(hdc, win::colorref(180, 180, 180));
    let font = GetStockObject(DEFAULT_GUI_FONT);
    let old_font = SelectObject(hdc, font);
    let (cap_w, cap_h) = capture_size_from_frame(sel_w, sel_h);
    let size_text = win::to_wide(&format!("{}×{}", cap_w, cap_h));
    TextOutW(hdc, geom.bar_l + 6, geom.bar_t + 8,
             size_text.as_ptr(), (size_text.len() - 1) as i32);
    SelectObject(hdc, old_font);

    let fx = geom.fx; let fy = geom.fy;

    // ── 프레임 테두리 (BORDER px) ────────────────────────────────────────
    let border_color = if recording { win::colorref(220, 50, 50) }
                       else         { win::colorref(255, 255, 255) };
    let bb = CreateSolidBrush(border_color);
    FillRect(hdc, &RECT { left: fx, top: fy, right: fx + sel_w, bottom: fy + BORDER }, bb);
    FillRect(hdc, &RECT { left: fx, top: fy + sel_h - BORDER, right: fx + sel_w, bottom: fy + sel_h }, bb);
    FillRect(hdc, &RECT { left: fx, top: fy + BORDER, right: fx + BORDER, bottom: fy + sel_h - BORDER }, bb);
    FillRect(hdc, &RECT { left: fx + sel_w - BORDER, top: fy + BORDER,
                          right: fx + sel_w, bottom: fy + sel_h - BORDER }, bb);
    DeleteObject(bb);

    EndPaint(hwnd, &ps);
}

// ═══════════════════════════════════════════════════════════════════════════
// 리사이즈 히트 테스트
// ═══════════════════════════════════════════════════════════════════════════

fn hit_test_resize(wx: i32, wy: i32, fx: i32, fy: i32, sel_w: i32, sel_h: i32) -> Drag {
    let rx = wx - fx;
    let ry = wy - fy;
    if rx < -SHADOW || rx > sel_w + SHADOW || ry < -SHADOW || ry > sel_h + SHADOW {
        return Drag::None;
    }
    if rx > BORDER + HANDLE && rx < sel_w - BORDER - HANDLE
        && ry > BORDER + HANDLE && ry < sel_h - BORDER - HANDLE
    {
        return Drag::None;
    }
    let nl = rx <= BORDER + HANDLE;
    let nr = rx >= sel_w - BORDER - HANDLE;
    let nt = ry <= BORDER + HANDLE;
    let nb = ry >= sel_h - BORDER - HANDLE;
    match (nl, nr, nt, nb) {
        (true, _, true, _) => Drag::NW,
        (_, true, true, _) => Drag::NE,
        (true, _, _, true) => Drag::SW,
        (_, true, _, true) => Drag::SE,
        (_, _, true, _)    => Drag::N,
        (_, _, _, true)    => Drag::S,
        (true, _, _, _)    => Drag::W,
        (_, true, _, _)    => Drag::E,
        _                  => Drag::None,
    }
}

fn cursor_for_drag(d: Drag) -> *const u16 {
    match d {
        Drag::N | Drag::S   => IDC_SIZENS,
        Drag::E | Drag::W   => IDC_SIZEWE,
        Drag::NW | Drag::SE => IDC_SIZENWSE,
        Drag::NE | Drag::SW => IDC_SIZENESW,
        Drag::None          => IDC_ARROW,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 마우스 이벤트
// ═══════════════════════════════════════════════════════════════════════════

unsafe fn on_mouse_down(hwnd: HWND, lp: LPARAM) {
    // 녹화 중에는 영역 변경 불가
    if crate::recorder::is_recording() { return; }

    let cx = (lp as usize & 0xffff) as i16 as i32;
    let cy = ((lp as usize >> 16) & 0xffff) as i16 as i32;
    let (sel_w, sel_h, bar_pos) = read_sel_dims_pos();
    let geom = compute_geom(sel_w, sel_h, bar_pos);
    let mode = hit_test_resize(cx, cy, geom.fx, geom.fy, sel_w, sel_h);
    if mode == Drag::None { return; }

    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    let start_sel = STATE.lock().expect("capture mutex poisoned")
        .as_ref().map(|s| s.sel).unwrap_or_else(|| unsafe { zeroed() });
    {
        let mut g = STATE.lock().expect("capture mutex poisoned");
        if let Some(s) = g.as_mut() {
            s.drag = Some(DragInfo { mode, start_cx: pt.x, start_cy: pt.y, start_sel });
        }
    }
    SetCapture(hwnd);
}

unsafe fn on_mouse_move(hwnd: HWND) {
    let drag = {
        let g = STATE.lock().expect("capture mutex poisoned");
        let s = match g.as_ref() { Some(s) => s, None => return };
        s.drag.as_ref().map(|d| (d.mode, d.start_cx, d.start_cy, d.start_sel))
    };
    let (mode, start_cx, start_cy, start_sel) = match drag { Some(d) => d, None => return };

    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    let dx = pt.x - start_cx;
    let dy = pt.y - start_cy;

    let mut sel = start_sel;
    match mode {
        Drag::N  => sel.top    = start_sel.top    + dy,
        Drag::S  => sel.bottom = start_sel.bottom + dy,
        Drag::W  => sel.left   = start_sel.left   + dx,
        Drag::E  => sel.right  = start_sel.right  + dx,
        Drag::NW => { sel.top  = start_sel.top  + dy; sel.left  = start_sel.left  + dx; }
        Drag::NE => { sel.top  = start_sel.top  + dy; sel.right = start_sel.right + dx; }
        Drag::SW => { sel.bottom = start_sel.bottom + dy; sel.left  = start_sel.left  + dx; }
        Drag::SE => { sel.bottom = start_sel.bottom + dy; sel.right = start_sel.right + dx; }
        Drag::None => return,
    }

    let aspect = STATE.lock().expect("capture mutex poisoned").as_ref().and_then(|s| s.aspect);
    if let Some((rw, rh)) = aspect {
        apply_aspect_constraint(&mut sel, mode, start_sel, rw, rh);
    }

    if sel.right - sel.left < MIN_SEL_W {
        match mode {
            Drag::W | Drag::NW | Drag::SW => sel.left  = sel.right  - MIN_SEL_W,
            _                             => sel.right = sel.left   + MIN_SEL_W,
        }
    }
    if sel.bottom - sel.top < MIN_SEL_H {
        match mode {
            Drag::N | Drag::NW | Drag::NE => sel.top    = sel.bottom - MIN_SEL_H,
            _                             => sel.bottom = sel.top    + MIN_SEL_H,
        }
    }

    let new_w = sel.right - sel.left;
    let new_h = sel.bottom - sel.top;
    let bar_pos = {
        let mut g = STATE.lock().expect("capture mutex poisoned");
        if let Some(s) = g.as_mut() { s.sel = sel; }
        g.as_ref().map(|s| s.bar_pos).unwrap_or(BarPos::Top)
    };

    let geom = compute_geom(new_w, new_h, bar_pos);
    let (wx, wy) = win_pos(&sel, bar_pos);
    SetWindowPos(hwnd, null_mut(), wx, wy, geom.win_w, geom.win_h,
                 SWP_NOZORDER | SWP_NOACTIVATE);

    update_ctrl_positions(hwnd);
    apply_region(hwnd);
    InvalidateRect(hwnd, null(), 1);
    UpdateWindow(hwnd);
}

unsafe fn on_mouse_up() {
    ReleaseCapture();
    let mut g = STATE.lock().expect("capture mutex poisoned");
    if let Some(s) = g.as_mut() { s.drag = None; }
}

fn apply_aspect_constraint(
    sel: &mut RECT, mode: Drag, start_sel: RECT, rw: i32, rh: i32,
) {
    if rw <= 0 || rh <= 0 { return; }
    let w = sel.right - sel.left;
    let h = sel.bottom - sel.top;
    match mode {
        Drag::E | Drag::W => {
            let new_h = w * rh / rw;
            let cy = (start_sel.top + start_sel.bottom) / 2;
            sel.top    = cy - new_h / 2;
            sel.bottom = cy + new_h / 2;
        }
        Drag::N | Drag::S => {
            let new_w = h * rw / rh;
            let cx = (start_sel.left + start_sel.right) / 2;
            sel.left  = cx - new_w / 2;
            sel.right = cx + new_w / 2;
        }
        Drag::NW | Drag::NE | Drag::SW | Drag::SE => {
            let dw = (w - (start_sel.right - start_sel.left)).abs();
            let dh = (h - (start_sel.bottom - start_sel.top)).abs();
            if dw * rh >= dh * rw {
                let new_h = w * rh / rw;
                match mode {
                    Drag::NW | Drag::NE => sel.top    = sel.bottom - new_h,
                    _                   => sel.bottom = sel.top    + new_h,
                }
            } else {
                let new_w = h * rw / rh;
                match mode {
                    Drag::NW | Drag::SW => sel.left  = sel.right - new_w,
                    _                   => sel.right = sel.left  + new_w,
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
    let (sel, config) = {
        let g = STATE.lock().expect("capture mutex poisoned");
        let s = match g.as_ref() { Some(s) => s, None => return };
        (s.sel, s.config.clone())
    };

    let mut st: windows_sys::Win32::Foundation::SYSTEMTIME = zeroed();
    GetLocalTime(&mut st);
    let filename = format!(
        "gif-{:04}{:02}{:02}-{:02}{:02}{:02}.gif",
        st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
    );
    let output_dir = std::path::PathBuf::from(&config.gif_output_path);
    let _ = std::fs::create_dir_all(&output_dir);
    let output_path = output_dir.join(&filename);
    let frame_w = sel.right - sel.left;
    let frame_h = sel.bottom - sel.top;
    let (cap_w, cap_h) = capture_size_from_frame(frame_w, frame_h);

    let region = crate::recorder::CaptureRegion {
        x:      sel.left   + BORDER,
        y:      sel.top    + BORDER,
        width:  cap_w,
        height: cap_h,
    };
    crate::recorder::start(
        region,
        output_path,
        config.show_gif_timestamp,
        config.gif_timestamp_font_size,
    );

    let (tb, cb) = {
        let g = STATE.lock().expect("capture mutex poisoned");
        let s = g.as_ref().expect("state missing");
        (s.toggle_btn as HWND, s.cancel_btn as HWND)
    };
    let stop_txt = win::to_wide("■");
    SetWindowTextW(tb, stop_txt.as_ptr());
    EnableWindow(cb, 0);

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
            s.cancelled   = false;
        }
    }
    _ = hwnd;
}

unsafe fn on_cancel(hwnd: HWND) {
    let is_active = crate::recorder::is_recording() || {
        STATE.lock().expect("capture mutex poisoned")
            .as_ref().map(|s| s.is_stopping).unwrap_or(false)
    };

    if is_active {
        crate::recorder::stop();
        let mut g = STATE.lock().expect("capture mutex poisoned");
        if let Some(s) = g.as_mut() {
            s.is_stopping = true;
            s.cancelled   = true;
        }
    } else {
        DestroyWindow(hwnd);
    }
}

unsafe fn on_preset_changed(hwnd: HWND) {
    let ctrl = {
        let g = STATE.lock().expect("capture mutex poisoned");
        g.as_ref().and_then(|s| if s.preset_ctrl != 0 { Some(s.preset_ctrl as HWND) } else { None })
    };
    let ctrl = match ctrl { Some(c) => c, None => return };
    let idx = SendMessageW(ctrl, CB_GETCURSEL, 0, 0) as i64;
    let (pw, ph): (i32, i32) = match idx {
        1 => (640,480), 2 => (1280,720), 3 => (1920,1080),
        4 => (1024,768), 5 => (800,600), _ => return,
    };
    let (new_aspect, ratio_idx): (Option<(i32,i32)>, usize) = match idx {
        1 | 4 | 5 => (Some((4, 3)), 2),
        2 | 3     => (Some((16, 9)), 3),
        _         => (None, 0),
    };
    let (frame_w, frame_h) = frame_size_from_capture(pw, ph);
    let ratio_ctrl_h = {
        let mut g = STATE.lock().expect("capture mutex poisoned");
        let s = match g.as_mut() { Some(s) => s, None => return };
        let cx = (s.sel.left + s.sel.right)  / 2;
        let cy = (s.sel.top  + s.sel.bottom) / 2;
        s.sel.left   = cx - frame_w / 2;
        s.sel.right  = s.sel.left + frame_w;
        s.sel.top    = cy - frame_h / 2;
        s.sel.bottom = s.sel.top + frame_h;
        s.aspect = new_aspect;
        s.ratio_ctrl as HWND
    };
    if !ratio_ctrl_h.is_null() {
        SendMessageW(ratio_ctrl_h, CB_SETCURSEL, ratio_idx, 0);
    }
    refresh_capture_window(hwnd);
    SendMessageW(ctrl, CB_SETCURSEL, 0, 0);
}

unsafe fn on_ratio_changed(hwnd: HWND) {
    let ctrl = {
        let g = STATE.lock().expect("capture mutex poisoned");
        g.as_ref().and_then(|s| if s.ratio_ctrl != 0 { Some(s.ratio_ctrl as HWND) } else { None })
    };
    let ctrl = match ctrl { Some(c) => c, None => return };
    let idx = SendMessageW(ctrl, CB_GETCURSEL, 0, 0) as i64;
    let aspect: Option<(i32,i32)> = match idx {
        1 => Some((1,1)), 2 => Some((4,3)), 3 => Some((16,9)),
        4 => Some((9,16)), _ => None,
    };
    {
        let mut g = STATE.lock().expect("capture mutex poisoned");
        let s = match g.as_mut() { Some(s) => s, None => return };
        s.aspect = aspect;

        if let Some((rw, rh)) = aspect {
            let frame_h = (s.sel.bottom - s.sel.top).max(MIN_SEL_H);
            let (_, cap_h) = capture_size_from_frame(s.sel.right - s.sel.left, frame_h);
            let min_cap_w = (MIN_SEL_W - BORDER * 2).max(1);
            let new_cap_w = (cap_h * rw / rh).max(min_cap_w);
            let (new_w, _) = frame_size_from_capture(new_cap_w, cap_h);
            let cx = (s.sel.left + s.sel.right) / 2;
            s.sel.left = cx - new_w / 2;
            s.sel.right = s.sel.left + new_w;
        }
    }
    refresh_capture_window(hwnd);
}

unsafe fn on_bar_pos_changed(hwnd: HWND) {
    let ctrl = {
        let g = STATE.lock().expect("capture mutex poisoned");
        g.as_ref().and_then(|s| if s.bar_pos_ctrl != 0 { Some(s.bar_pos_ctrl as HWND) } else { None })
    };
    let ctrl = match ctrl { Some(c) => c, None => return };
    let idx = SendMessageW(ctrl, CB_GETCURSEL, 0, 0) as i64;
    let new_pos = match idx {
        0 => BarPos::Top,
        _ => BarPos::Bottom,
    };
    {
        let mut g = STATE.lock().expect("capture mutex poisoned");
        if let Some(s) = g.as_mut() { s.bar_pos = new_pos; }
    }
    reposition_window(hwnd);
}

unsafe fn reposition_window(hwnd: HWND) {
    auto_adjust_bar_pos();
    let (sel, bar_pos) = {
        let g = STATE.lock().expect("capture mutex poisoned");
        let s = match g.as_ref() { Some(s) => s, None => return };
        (s.sel, s.bar_pos)
    };
    let sel_w = sel.right - sel.left;
    let sel_h = sel.bottom - sel.top;
    let geom  = compute_geom(sel_w, sel_h, bar_pos);
    let (wx, wy) = win_pos(&sel, bar_pos);
    SetWindowPos(hwnd, null_mut(), wx, wy, geom.win_w, geom.win_h,
                 SWP_NOZORDER | SWP_NOACTIVATE);
    rebuild_controls(hwnd);
    apply_region(hwnd);
    InvalidateRect(hwnd, null(), 1);
    UpdateWindow(hwnd);
}

unsafe fn refresh_capture_window(hwnd: HWND) {
    auto_adjust_bar_pos();
    let (sel, bar_pos) = {
        let g = STATE.lock().expect("capture mutex poisoned");
        let s = match g.as_ref() { Some(s) => s, None => return };
        (s.sel, s.bar_pos)
    };
    let sel_w = sel.right - sel.left;
    let sel_h = sel.bottom - sel.top;
    let geom = compute_geom(sel_w, sel_h, bar_pos);
    let (wx, wy) = win_pos(&sel, bar_pos);
    SetWindowPos(hwnd, null_mut(), wx, wy, geom.win_w, geom.win_h,
                 SWP_NOZORDER | SWP_NOACTIVATE);
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
    let top_over = s.sel.top - BAR_H < screen_top;
    let bottom_over = s.sel.top + sel_h + BAR_H > screen_bottom;

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
    let (is_stopping, cancelled) = STATE.lock().expect("capture mutex poisoned")
        .as_ref().map(|s| (s.is_stopping, s.cancelled)).unwrap_or((false, false));
    if !is_stopping || crate::recorder::is_recording() { return; }

    KillTimer(hwnd, TIMER_POLL);

    let (tb, cb) = {
        let g = STATE.lock().expect("capture mutex poisoned");
        let s = match g.as_ref() { Some(s) => s, None => return };
        (s.toggle_btn as HWND, s.cancel_btn as HWND)
    };
    let rec_txt = win::to_wide("●");
    SetWindowTextW(tb, rec_txt.as_ptr());
    EnableWindow(cb, 1);

    {
        let mut g = STATE.lock().expect("capture mutex poisoned");
        if let Some(s) = g.as_mut() { s.is_stopping = false; }
    }

    if cancelled {
        let output_path = crate::recorder::last_output_path();
        let _ = std::fs::remove_file(&output_path);
        DestroyWindow(hwnd);
        return;
    }

    InvalidateRect(hwnd, null(), 1);
    UpdateWindow(hwnd);

    let (open_folder, output_path) = {
        let g = STATE.lock().expect("capture mutex poisoned");
        let s = g.as_ref().expect("state missing");
        (s.config.open_folder_after_save, crate::recorder::last_output_path())
    };
    if open_folder {
        if let Some(folder) = output_path.parent() {
            let path_w = win::to_wide(&folder.to_string_lossy());
            let op_w   = win::to_wide("explore");
            ShellExecuteW(null_mut(), op_w.as_ptr(), path_w.as_ptr(),
                          null_mut(), null_mut(), SW_SHOWDEFAULT);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 유틸리티
// ═══════════════════════════════════════════════════════════════════════════

fn read_sel_dims_pos() -> (i32, i32, BarPos) {
    let g = STATE.lock().expect("capture mutex poisoned");
    match g.as_ref() {
        Some(s) => (s.sel.right - s.sel.left, s.sel.bottom - s.sel.top, s.bar_pos),
        None    => (MIN_SEL_W, MIN_SEL_H, BarPos::Top),
    }
}

fn capture_size_from_frame(frame_w: i32, frame_h: i32) -> (i32, i32) {
    (
        (frame_w - BORDER * 2).max(1),
        (frame_h - BORDER * 2).max(1),
    )
}

fn frame_size_from_capture(cap_w: i32, cap_h: i32) -> (i32, i32) {
    (
        (cap_w + BORDER * 2).max(MIN_SEL_W),
        (cap_h + BORDER * 2).max(MIN_SEL_H),
    )
}
