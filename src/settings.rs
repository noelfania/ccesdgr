use std::mem::size_of;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateSolidBrush, DeleteObject, FillRect, FrameRect, GetStockObject, InvalidateRect,
    UpdateWindow, COLOR_3DFACE, DEFAULT_GUI_FONT, HDC,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::Dialogs::{
    ChooseColorW, CC_FULLOPEN, CC_RGBINIT, CC_SOLIDCOLOR, CHOOSECOLORW,
};
use windows_sys::Win32::UI::Controls::{
    CheckDlgButton, IsDlgButtonChecked, BST_CHECKED, BST_UNCHECKED, DRAWITEMSTRUCT,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetDlgItem, GetWindowTextLengthW,
    GetWindowTextW, LoadCursorW, LoadIconW, MessageBoxW, RegisterClassExW, SendMessageW,
    SetWindowTextW, ShowWindow, BS_AUTOCHECKBOX, BS_DEFPUSHBUTTON, BS_GROUPBOX, BS_OWNERDRAW,
    BS_PUSHBUTTON, CW_USEDEFAULT, ES_AUTOHSCROLL, HMENU, IDC_ARROW, MB_ICONERROR, MB_OK, SW_SHOW,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_DRAWITEM, WM_HSCROLL,
    WM_SETFONT, WNDCLASSEXW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_OVERLAPPED, WS_SYSMENU,
    WS_TABSTOP, WS_VISIBLE,
};

use crate::config::{
    AppConfig, ClickColor, ClickEffectKind, MAX_GIF_TIMESTAMP_FONT_SIZE, MAX_RIPPLE_LIFETIME,
    MAX_RIPPLE_RADIUS, MIN_GIF_TIMESTAMP_FONT_SIZE, MIN_RIPPLE_LIFETIME, MIN_RIPPLE_RADIUS,
};
use crate::win;

const CLASS_NAME: &str = "GMTLMouseClickSettingsWindow";
const ID_LEFT_COLOR: i32 = 2001;
const ID_RIGHT_COLOR: i32 = 2002;
const ID_STARTUP_CHECK: i32 = 2003;
const ID_RIPPLE_RADIUS: i32 = 2004;
const ID_CONFIRM: i32 = 2005;
const ID_RESET: i32 = 2006;
const ID_CANCEL: i32 = 2007;
const ID_APPLY: i32 = 2008;
const ID_GIF_PATH: i32 = 2010;
const ID_GIF_OPEN_FOLDER: i32 = 2011;
const ID_GIF_TIMESTAMP: i32 = 2012;
const ID_GIF_TIMESTAMP_SIZE: i32 = 2013;
const ID_RIPPLE_SPEED: i32 = 2014;
const ID_CLICK_EFFECT_KIND: i32 = 2015;
const ID_LEFT_HEX: i32 = 2016;
const ID_RIGHT_HEX: i32 = 2017;
const ID_PREVIEW_LEFT: i32 = 2018;
const ID_PREVIEW_RIGHT: i32 = 2019;
const ID_RIPPLE_SPEED_VALUE: i32 = 2026;
// 최근 색상 스와치 (각 색상 타입별 3개)
const ID_SWATCH_LEFT_0: i32 = 2020;
const ID_SWATCH_RIGHT_0: i32 = 2023;
const ID_NAV_GENERAL: i32 = 2101;
const ID_NAV_MOUSE: i32 = 2102;
const ID_NAV_INFO: i32 = 2103;

// 트랙바 메시지 상수 (WM_USER 기반)
const TBM_GETPOS: u32 = 0x0400;
const TBM_SETRANGE: u32 = 0x0406;
const TBM_SETPOS: u32 = 0x0405;
const TBS_AUTOTICKS: WINDOW_STYLE = 0x0001;
const CB_ADDSTRING: u32 = 0x0143;
const CB_SETCURSEL: u32 = 0x014E;
const CB_GETCURSEL: u32 = 0x0147;
const CBS_DROPDOWNLIST: WINDOW_STYLE = 3;

const PAGE_GENERAL: i32 = 1;
const PAGE_MOUSE: i32 = 2;
const PAGE_INFO: i32 = 3;

static SETTINGS_HWND: AtomicIsize = AtomicIsize::new(0);
static PENDING_CONFIG: Mutex<Option<AppConfig>> = Mutex::new(None);
static CURRENT_CONFIG: Mutex<Option<AppConfig>> = Mutex::new(None);
static CUSTOM_COLORS: Mutex<[COLORREF; 16]> = Mutex::new([0; 16]);
static CURRENT_PAGE: Mutex<i32> = Mutex::new(PAGE_GENERAL);
static CONTENT_CONTROLS: Mutex<Vec<isize>> = Mutex::new(Vec::new());

pub fn show(owner: HWND, config: &AppConfig) {
    if let Some(hwnd) = active_window() {
        unsafe {
            ShowWindow(hwnd, SW_SHOW);
            SetFocus(hwnd);
        }
        return;
    }

    *PENDING_CONFIG.lock().expect("settings mutex poisoned") = Some(config.clone());

    let instance = unsafe { GetModuleHandleW(null()) };
    let class_name = win::to_wide(CLASS_NAME);
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(settings_proc),
        hInstance: instance,
        hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
        hIcon: unsafe { LoadIconW(instance, win::resource_id(win::APP_ICON_ID)) },
        hIconSm: unsafe { LoadIconW(instance, win::resource_id(win::APP_ICON_ID)) },
        hbrBackground: (COLOR_3DFACE + 1) as isize as _,
        lpszClassName: class_name.as_ptr(),
        ..unsafe { std::mem::zeroed() }
    };

    unsafe {
        RegisterClassExW(&class);
    }

    let title = win::to_wide("GMTL Mouse Click 설정");
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            620,
            445,
            owner,
            null_mut(),
            instance,
            null_mut(),
        )
    };

    if !hwnd.is_null() {
        SETTINGS_HWND.store(hwnd as isize, Ordering::Relaxed);
        unsafe {
            ShowWindow(hwnd, SW_SHOW);
            SetFocus(hwnd);
            EnableWindow(owner, 0);
        }
    }
}

unsafe extern "system" fn settings_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CREATE => {
            create_controls(hwnd);
            0
        }
        WM_DRAWITEM => {
            let dis = &*(lparam as *const DRAWITEMSTRUCT);
            let id = dis.CtlID as i32;
            if id == ID_PREVIEW_LEFT || id == ID_PREVIEW_RIGHT {
                draw_color_preview(dis.hDC, id, &dis.rcItem);
                return 1;
            }
            if (ID_SWATCH_LEFT_0..=ID_SWATCH_RIGHT_0 + 2).contains(&id) {
                draw_swatch(dis.hDC, id, &dis.rcItem);
                return 1;
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_COMMAND => {
            let command_id = win::low_word(wparam as usize) as i32;
            match command_id {
                ID_NAV_GENERAL => {
                    show_page(hwnd, PAGE_GENERAL);
                    0
                }
                ID_NAV_MOUSE => {
                    show_page(hwnd, PAGE_MOUSE);
                    0
                }
                ID_NAV_INFO => {
                    show_page(hwnd, PAGE_INFO);
                    0
                }
                ID_LEFT_COLOR => {
                    if !sync_current_page(hwnd) {
                        return 0;
                    }
                    choose_click_color(hwnd, ID_LEFT_COLOR);
                    refresh_if_mouse(hwnd);
                    0
                }
                ID_RIGHT_COLOR => {
                    if !sync_current_page(hwnd) {
                        return 0;
                    }
                    choose_click_color(hwnd, ID_RIGHT_COLOR);
                    refresh_if_mouse(hwnd);
                    0
                }
                // 최근 색상 스와치 클릭
                id if (ID_SWATCH_LEFT_0..=ID_SWATCH_RIGHT_0 + 2).contains(&id) => {
                    if !sync_current_page(hwnd) {
                        return 0;
                    }
                    apply_swatch_color(hwnd, id);
                    refresh_if_mouse(hwnd);
                    0
                }
                ID_RESET => {
                    reset_settings(hwnd);
                    0
                }
                ID_CONFIRM => {
                    save_settings(hwnd);
                    0
                }
                ID_CANCEL => {
                    DestroyWindow(hwnd);
                    0
                }
                ID_APPLY => {
                    apply_settings_from_window(hwnd);
                    0
                }
                _ => DefWindowProcW(hwnd, message, wparam, lparam),
            }
        }
        WM_HSCROLL => {
            if *CURRENT_PAGE.lock().expect("settings page mutex poisoned") == PAGE_MOUSE {
                update_speed_text_from_slider(hwnd);
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_DESTROY => {
            if SETTINGS_HWND.load(Ordering::Relaxed) == hwnd as isize {
                SETTINGS_HWND.store(0, Ordering::Relaxed);
            }
            let owner = windows_sys::Win32::UI::WindowsAndMessaging::GetWindow(
                hwnd,
                windows_sys::Win32::UI::WindowsAndMessaging::GW_OWNER,
            );
            if !owner.is_null() {
                EnableWindow(owner, 1);
                SetFocus(owner);
            }
            *CURRENT_CONFIG.lock().expect("settings mutex poisoned") = None;
            CONTENT_CONTROLS
                .lock()
                .expect("settings controls mutex poisoned")
                .clear();
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn refresh_if_mouse(hwnd: HWND) {
    if *CURRENT_PAGE.lock().expect("settings page mutex poisoned") == PAGE_MOUSE {
        render_page(hwnd, PAGE_MOUSE);
    }
}

unsafe fn create_controls(hwnd: HWND) {
    let config = PENDING_CONFIG
        .lock()
        .expect("settings mutex poisoned")
        .take()
        .unwrap_or_default();

    *CURRENT_CONFIG.lock().expect("settings mutex poisoned") = Some(config.clone());
    *CURRENT_PAGE.lock().expect("settings page mutex poisoned") = PAGE_GENERAL;

    button(hwnd, ID_NAV_GENERAL, "기본", 16, 14, 72, 28, false);
    button(hwnd, ID_NAV_MOUSE, "마우스", 88, 14, 82, 28, false);
    button(hwnd, ID_NAV_INFO, "정보", 170, 14, 72, 28, false);

    button(hwnd, ID_RESET, "초기화", 16, 374, 86, 28, false);
    button(hwnd, ID_CONFIRM, "확인", 320, 374, 78, 28, true);
    button(hwnd, ID_CANCEL, "취소", 406, 374, 78, 28, false);
    button(hwnd, ID_APPLY, "적용", 492, 374, 78, 28, false);

    show_page(hwnd, PAGE_GENERAL);
}

unsafe fn save_settings(hwnd: HWND) {
    if !apply_settings_from_window(hwnd) {
        return;
    }
    DestroyWindow(hwnd);
}

unsafe fn apply_settings_from_window(hwnd: HWND) -> bool {
    if !sync_current_page(hwnd) {
        return false;
    }

    let Some(config) = CURRENT_CONFIG
        .lock()
        .expect("settings mutex poisoned")
        .clone()
    else {
        error(hwnd, "설정 상태를 읽을 수 없습니다.");
        return false;
    };

    if let Err(message) = crate::apply_settings(config) {
        error(hwnd, &message);
        return false;
    }

    true
}

unsafe fn reset_settings(hwnd: HWND) {
    let default_config = AppConfig::default();
    *CURRENT_CONFIG.lock().expect("settings mutex poisoned") = Some(default_config);
    let page = *CURRENT_PAGE.lock().expect("settings page mutex poisoned");
    render_page(hwnd, page);
}

unsafe fn show_page(hwnd: HWND, page: i32) {
    if !sync_current_page(hwnd) {
        return;
    }
    render_page(hwnd, page);
}

unsafe fn render_page(hwnd: HWND, page: i32) {
    *CURRENT_PAGE.lock().expect("settings page mutex poisoned") = page;
    clear_content_controls(hwnd);

    match page {
        PAGE_MOUSE => create_mouse_page(hwnd),
        PAGE_INFO => create_info_page(hwnd),
        _ => create_general_page(hwnd),
    }

    repaint_window(hwnd);
}

unsafe fn sync_current_page(hwnd: HWND) -> bool {
    let page = *CURRENT_PAGE.lock().expect("settings page mutex poisoned");
    let has_content = !CONTENT_CONTROLS
        .lock()
        .expect("settings controls mutex poisoned")
        .is_empty();
    if !has_content {
        return true;
    }

    if let Some(config) = CURRENT_CONFIG
        .lock()
        .expect("settings mutex poisoned")
        .as_mut()
    {
        match page {
            PAGE_GENERAL => {
                config.start_with_windows =
                    IsDlgButtonChecked(hwnd, ID_STARTUP_CHECK) == BST_CHECKED;
                let path = control_text(hwnd, ID_GIF_PATH);
                if !path.is_empty() {
                    config.gif_output_path = path;
                }
                config.open_folder_after_save =
                    IsDlgButtonChecked(hwnd, ID_GIF_OPEN_FOLDER) == BST_CHECKED;
                config.show_gif_timestamp =
                    IsDlgButtonChecked(hwnd, ID_GIF_TIMESTAMP) == BST_CHECKED;
                if let Ok(size) = control_text(hwnd, ID_GIF_TIMESTAMP_SIZE)
                    .trim()
                    .parse::<i32>()
                {
                    config.gif_timestamp_font_size =
                        size.clamp(MIN_GIF_TIMESTAMP_FONT_SIZE, MAX_GIF_TIMESTAMP_FONT_SIZE);
                }
            }
            PAGE_MOUSE => {
                let Some(left_color) = read_hex_color(hwnd, ID_LEFT_HEX, "왼쪽 클릭 효과 색상")
                else {
                    return false;
                };
                let Some(right_color) = read_hex_color(hwnd, ID_RIGHT_HEX, "오른쪽 클릭 효과 색상")
                else {
                    return false;
                };
                config.left_click_color = left_color;
                config.right_click_color = right_color;

                let effect_combo = GetDlgItem(hwnd, ID_CLICK_EFFECT_KIND);
                if !effect_combo.is_null() {
                    config.click_effect_kind = match SendMessageW(effect_combo, CB_GETCURSEL, 0, 0)
                    {
                        1 => ClickEffectKind::FilledFade,
                        _ => ClickEffectKind::Ripple,
                    };
                }
                if let Ok(radius) = control_text(hwnd, ID_RIPPLE_RADIUS).trim().parse::<i32>() {
                    config.ripple_radius = radius.clamp(MIN_RIPPLE_RADIUS, MAX_RIPPLE_RADIUS);
                }
                let speed_from_text = control_text(hwnd, ID_RIPPLE_SPEED_VALUE)
                    .trim()
                    .parse::<u32>()
                    .ok()
                    .map(|speed| speed.clamp(MIN_RIPPLE_LIFETIME, MAX_RIPPLE_LIFETIME));

                if let Some(speed) = speed_from_text {
                    config.ripple_lifetime = lifetime_from_speed(speed);
                    let track = GetDlgItem(hwnd, ID_RIPPLE_SPEED);
                    if !track.is_null() {
                        SendMessageW(track, TBM_SETPOS, 1, speed as LPARAM);
                    }
                } else {
                    // 트랙바 값 읽기 (슬라이더가 오른쪽일수록 빠름 = 낮은 lifetime)
                    let track = GetDlgItem(hwnd, ID_RIPPLE_SPEED);
                    if !track.is_null() {
                        let slider_pos = SendMessageW(track, TBM_GETPOS, 0, 0) as u32;
                        // 슬라이더 pos ↑ → 빠름 → lifetime ↓: lifetime = MAX+MIN-pos
                        config.ripple_lifetime = lifetime_from_speed(slider_pos);
                    }
                }
            }
            _ => {}
        }
    }

    true
}

unsafe fn create_general_page(hwnd: HWND) {
    let config = CURRENT_CONFIG
        .lock()
        .expect("settings mutex poisoned")
        .clone()
        .unwrap_or_default();

    content_group_box(hwnd, "기본 설정", 16, 52, 554, 288);

    content_label(hwnd, "윈도우 실행시 기동", 42, 92, 180, 24);
    content_label(
        hwnd,
        "체크하면 다음 Windows 로그인부터 자동으로 실행됩니다.",
        42,
        118,
        420,
        22,
    );
    let startup_cb = content_checkbox(hwnd, ID_STARTUP_CHECK, "", 512, 92, 40, 24);
    CheckDlgButton(
        hwnd,
        ID_STARTUP_CHECK,
        if config.start_with_windows {
            BST_CHECKED
        } else {
            BST_UNCHECKED
        },
    );
    remember_content(startup_cb);

    content_label(hwnd, "GIF 저장 폴더", 42, 162, 140, 24);
    content_edit(hwnd, ID_GIF_PATH, &config.gif_output_path, 42, 188, 490, 24);

    content_label(hwnd, "저장 후 폴더 열기", 42, 228, 200, 24);
    let open_cb = content_checkbox(hwnd, ID_GIF_OPEN_FOLDER, "", 512, 228, 40, 24);
    CheckDlgButton(
        hwnd,
        ID_GIF_OPEN_FOLDER,
        if config.open_folder_after_save {
            BST_CHECKED
        } else {
            BST_UNCHECKED
        },
    );
    remember_content(open_cb);

    content_label(hwnd, "GIF에 경과시간 표시", 42, 270, 200, 24);
    let timestamp_cb = content_checkbox(hwnd, ID_GIF_TIMESTAMP, "", 512, 270, 40, 24);
    CheckDlgButton(
        hwnd,
        ID_GIF_TIMESTAMP,
        if config.show_gif_timestamp {
            BST_CHECKED
        } else {
            BST_UNCHECKED
        },
    );
    remember_content(timestamp_cb);

    content_label(hwnd, "경과시간 글자 크기 (8-24)", 42, 308, 220, 24);
    content_edit(
        hwnd,
        ID_GIF_TIMESTAMP_SIZE,
        &config.gif_timestamp_font_size.to_string(),
        512,
        304,
        40,
        24,
    );
}

unsafe fn create_mouse_page(hwnd: HWND) {
    let config = CURRENT_CONFIG
        .lock()
        .expect("settings mutex poisoned")
        .clone()
        .unwrap_or_default();

    content_group_box(hwnd, "마우스 설정", 16, 52, 554, 300);

    content_label(hwnd, "클릭 효과", 42, 92, 120, 24);
    let effect_combo = content_combo(hwnd, ID_CLICK_EFFECT_KIND, 240, 88, 180, 120);
    combo_add(effect_combo, "물결파장");
    combo_add(effect_combo, "채워진 원 페이드");
    let effect_index = match config.click_effect_kind {
        ClickEffectKind::Ripple => 0,
        ClickEffectKind::FilledFade => 1,
    };
    SendMessageW(effect_combo, CB_SETCURSEL, effect_index, 0);

    // ── 왼쪽 클릭 색상 (y=128) ──────────────────────────────────────
    content_label(hwnd, "왼쪽 클릭 효과 색상", 42, 128, 190, 24);
    content_color_preview(hwnd, ID_PREVIEW_LEFT, 240, 124, 28, 24);
    content_edit(
        hwnd,
        ID_LEFT_HEX,
        &config.left_click_color.to_hex(),
        276,
        124,
        78,
        24,
    );
    content_swatches(hwnd, ID_SWATCH_LEFT_0, 362, 127);
    content_button(hwnd, ID_LEFT_COLOR, "색상 선택", 442, 124, 96, 26, false);

    // ── 오른쪽 클릭 색상 (y=172) ──────────────────────────────────
    content_label(hwnd, "오른쪽 클릭 효과 색상", 42, 172, 190, 24);
    content_color_preview(hwnd, ID_PREVIEW_RIGHT, 240, 168, 28, 24);
    content_edit(
        hwnd,
        ID_RIGHT_HEX,
        &config.right_click_color.to_hex(),
        276,
        168,
        78,
        24,
    );
    content_swatches(hwnd, ID_SWATCH_RIGHT_0, 362, 171);
    content_button(hwnd, ID_RIGHT_COLOR, "색상 선택", 442, 168, 96, 26, false);

    // ── 효과 반지름 (y=216) ─────────────────────────────────────
    content_label(hwnd, "효과 반지름", 42, 216, 180, 24);
    content_edit(
        hwnd,
        ID_RIPPLE_RADIUS,
        &config.ripple_radius.to_string(),
        240,
        212,
        72,
        24,
    );
    content_label(
        hwnd,
        &format!("{} ~ {} px", MIN_RIPPLE_RADIUS, MAX_RIPPLE_RADIUS),
        322,
        216,
        130,
        24,
    );

    // ── 효과 속도 (y=252) ────────────────────────────────────────
    content_label(hwnd, "효과 속도", 42, 256, 100, 24);
    content_label(hwnd, "느림", 146, 283, 30, 20);
    // 트랙바: 느림←→빠름 (슬라이더 값 ↑ = 빠름 = lifetime ↓)
    let track_cls = win::to_wide("msctls_trackbar32");
    let track = CreateWindowExW(
        0,
        track_cls.as_ptr(),
        null(),
        WS_CHILD | WS_VISIBLE | TBS_AUTOTICKS,
        178,
        274,
        250,
        26,
        hwnd,
        ID_RIPPLE_SPEED as isize as HMENU,
        null_mut(),
        null_mut(),
    );
    if !track.is_null() {
        // range: 10(느림,좌)~60(빠름,우) → 반전: 슬라이더 위치 = MAX+MIN-lifetime
        let range_lp = ((MAX_RIPPLE_LIFETIME as i32) << 16 | MIN_RIPPLE_LIFETIME as i32) as LPARAM;
        SendMessageW(track, TBM_SETRANGE, 1, range_lp);
        let slider_pos = speed_from_lifetime(config.ripple_lifetime) as LPARAM;
        SendMessageW(track, TBM_SETPOS, 1, slider_pos);
        remember_content(track);
    }
    content_edit(
        hwnd,
        ID_RIPPLE_SPEED_VALUE,
        &speed_from_lifetime(config.ripple_lifetime).to_string(),
        462,
        252,
        50,
        24,
    );
    content_label(hwnd, "빠름", 432, 283, 30, 20);
    content_label(hwnd, "10 ~ 60", 516, 256, 48, 20);
}

unsafe fn create_info_page(hwnd: HWND) {
    content_group_box(hwnd, "프로그램 정보", 16, 52, 554, 218);
    content_label(hwnd, "GMTL Mouse Click", 42, 92, 220, 24);
    content_label(
        hwnd,
        "마우스 클릭 위치에 원형 파장 효과를 표시합니다.",
        42,
        126,
        420,
        24,
    );
    content_label(
        hwnd,
        "트레이 아이콘에서 설정창을 다시 열거나 종료할 수 있습니다.",
        42,
        156,
        460,
        24,
    );
}

// 스와치 색상 조회 (CURRENT_CONFIG의 recent 목록에서)
fn swatch_color(id: i32) -> Option<ClickColor> {
    let g = CURRENT_CONFIG.lock().ok()?;
    let config = g.as_ref()?;
    let (list, idx) = match id {
        2020..=2022 => (&config.recent_left_colors, (id - ID_SWATCH_LEFT_0) as usize),
        2023..=2025 => (
            &config.recent_right_colors,
            (id - ID_SWATCH_RIGHT_0) as usize,
        ),
        _ => return None,
    };
    list.get(idx).copied()
}

fn preview_color(id: i32) -> Option<ClickColor> {
    let g = CURRENT_CONFIG.lock().ok()?;
    let config = g.as_ref()?;
    match id {
        ID_PREVIEW_LEFT => Some(config.left_click_color),
        ID_PREVIEW_RIGHT => Some(config.right_click_color),
        _ => None,
    }
}

unsafe fn draw_color_preview(hdc: HDC, id: i32, rc: &windows_sys::Win32::Foundation::RECT) {
    let color = preview_color(id).unwrap_or_else(|| ClickColor::new(180, 180, 180));
    let brush = CreateSolidBrush(color.to_colorref());
    FillRect(hdc, rc, brush);
    DeleteObject(brush);

    let border = CreateSolidBrush(win::colorref(80, 80, 80));
    FrameRect(hdc, rc, border);
    DeleteObject(border);
}

// 스와치 버튼 그리기 (WM_DRAWITEM에서 호출)
unsafe fn draw_swatch(hdc: HDC, id: i32, rc: &windows_sys::Win32::Foundation::RECT) {
    if let Some(color) = swatch_color(id) {
        let brush = CreateSolidBrush(color.to_colorref());
        FillRect(hdc, rc, brush);
        DeleteObject(brush);
    } else {
        // 색상 없음: 회색 채우기
        let brush = CreateSolidBrush(win::colorref(180, 180, 180));
        FillRect(hdc, rc, brush);
        DeleteObject(brush);
    }
    // 테두리
    let border = CreateSolidBrush(win::colorref(100, 100, 100));
    FrameRect(hdc, rc, border);
    DeleteObject(border);
}

// 스와치 클릭 시 해당 색상을 현재 설정에 반영
unsafe fn apply_swatch_color(hwnd: HWND, id: i32) {
    let color = match swatch_color(id) {
        Some(c) => c,
        None => return,
    };
    let mut g = CURRENT_CONFIG.lock().expect("settings mutex poisoned");
    let config = match g.as_mut() {
        Some(c) => c,
        None => return,
    };

    if (ID_SWATCH_LEFT_0..=ID_SWATCH_LEFT_0 + 2).contains(&id) {
        config.left_click_color = color;
    } else if (ID_SWATCH_RIGHT_0..=ID_SWATCH_RIGHT_0 + 2).contains(&id) {
        config.right_click_color = color;
    }
    _ = hwnd;
}

// 최근 색상 목록 갱신 (중복 제거 후 최대 3개 유지)
fn push_recent(recent: &mut Vec<ClickColor>, color: ClickColor) {
    recent.retain(|c| c.to_colorref() != color.to_colorref());
    recent.insert(0, color);
    recent.truncate(3);
}

fn speed_from_lifetime(lifetime: u32) -> u32 {
    (MIN_RIPPLE_LIFETIME + MAX_RIPPLE_LIFETIME)
        .saturating_sub(lifetime)
        .clamp(MIN_RIPPLE_LIFETIME, MAX_RIPPLE_LIFETIME)
}

fn lifetime_from_speed(speed: u32) -> u32 {
    (MIN_RIPPLE_LIFETIME + MAX_RIPPLE_LIFETIME)
        .saturating_sub(speed)
        .clamp(MIN_RIPPLE_LIFETIME, MAX_RIPPLE_LIFETIME)
}

unsafe fn update_speed_text_from_slider(hwnd: HWND) {
    let track = GetDlgItem(hwnd, ID_RIPPLE_SPEED);
    if track.is_null() {
        return;
    }

    let speed = (SendMessageW(track, TBM_GETPOS, 0, 0) as u32)
        .clamp(MIN_RIPPLE_LIFETIME, MAX_RIPPLE_LIFETIME);
    set_control_text(hwnd, ID_RIPPLE_SPEED_VALUE, &speed.to_string());
}

// 색상 선택 다이얼로그 (왼쪽/오른쪽/외곽선 공통)
unsafe fn choose_click_color(hwnd: HWND, target: i32) {
    let Some(mut config) = CURRENT_CONFIG
        .lock()
        .expect("settings mutex poisoned")
        .clone()
    else {
        return;
    };

    let initial_color = match target {
        ID_LEFT_COLOR => config.left_click_color,
        ID_RIGHT_COLOR => config.right_click_color,
        _ => return,
    };

    let mut custom_colors = CUSTOM_COLORS.lock().expect("custom colors mutex poisoned");
    let mut dialog = CHOOSECOLORW {
        lStructSize: size_of::<CHOOSECOLORW>() as u32,
        hwndOwner: hwnd,
        rgbResult: initial_color.to_colorref(),
        lpCustColors: custom_colors.as_mut_ptr(),
        Flags: CC_RGBINIT | CC_FULLOPEN | CC_SOLIDCOLOR,
        ..std::mem::zeroed()
    };

    if ChooseColorW(&mut dialog) == 0 {
        return;
    }

    let new_color = ClickColor::from_colorref(dialog.rgbResult);
    match target {
        ID_LEFT_COLOR => {
            push_recent(&mut config.recent_left_colors, config.left_click_color);
            config.left_click_color = new_color;
        }
        ID_RIGHT_COLOR => {
            push_recent(&mut config.recent_right_colors, config.right_click_color);
            config.right_click_color = new_color;
        }
        _ => {}
    }

    *CURRENT_CONFIG.lock().expect("settings mutex poisoned") = Some(config);
}

// ── 헬퍼: 스와치 3개 생성 ───────────────────────────────────────────
unsafe fn content_swatches(hwnd: HWND, base_id: i32, x: i32, y: i32) {
    for i in 0..3 {
        let sw = child(
            hwnd,
            "BUTTON",
            "",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as WINDOW_STYLE,
            0,
            x + i * 22,
            y,
            18,
            18,
            base_id + i,
        );
        remember_content(sw);
    }
}

unsafe fn clear_content_controls(parent: HWND) {
    let mut controls = CONTENT_CONTROLS
        .lock()
        .expect("settings controls mutex poisoned");
    for hwnd in controls.drain(..) {
        DestroyWindow(hwnd as HWND);
    }
    repaint_window(parent);
}

unsafe fn repaint_window(hwnd: HWND) {
    InvalidateRect(hwnd, null(), 1);
    UpdateWindow(hwnd);
}

fn remember_content(hwnd: HWND) {
    if !hwnd.is_null() {
        CONTENT_CONTROLS
            .lock()
            .expect("settings controls mutex poisoned")
            .push(hwnd as isize);
    }
}

unsafe fn content_label(hwnd: HWND, text: &str, x: i32, y: i32, width: i32, height: i32) {
    let control = child(
        hwnd,
        "STATIC",
        text,
        WS_CHILD | WS_VISIBLE,
        0,
        x,
        y,
        width,
        height,
        0,
    );
    remember_content(control);
}

unsafe fn content_group_box(hwnd: HWND, text: &str, x: i32, y: i32, width: i32, height: i32) {
    let control = child(
        hwnd,
        "BUTTON",
        text,
        WS_CHILD | WS_VISIBLE | BS_GROUPBOX as WINDOW_STYLE,
        0,
        x,
        y,
        width,
        height,
        0,
    );
    remember_content(control);
}

unsafe fn content_checkbox(
    hwnd: HWND,
    id: i32,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> HWND {
    child(
        hwnd,
        "BUTTON",
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX as WINDOW_STYLE,
        0,
        x,
        y,
        width,
        height,
        id,
    )
}

unsafe fn content_edit(hwnd: HWND, id: i32, text: &str, x: i32, y: i32, width: i32, height: i32) {
    let control = child(
        hwnd,
        "EDIT",
        text,
        WS_CHILD | WS_VISIBLE | WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL as WINDOW_STYLE,
        0,
        x,
        y,
        width,
        height,
        id,
    );
    remember_content(control);
}

unsafe fn content_color_preview(hwnd: HWND, id: i32, x: i32, y: i32, width: i32, height: i32) {
    let control = child(
        hwnd,
        "BUTTON",
        "",
        WS_CHILD | WS_VISIBLE | BS_OWNERDRAW as WINDOW_STYLE,
        0,
        x,
        y,
        width,
        height,
        id,
    );
    remember_content(control);
}

unsafe fn content_combo(hwnd: HWND, id: i32, x: i32, y: i32, width: i32, height: i32) -> HWND {
    let control = child(
        hwnd,
        "COMBOBOX",
        "",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | CBS_DROPDOWNLIST,
        0,
        x,
        y,
        width,
        height,
        id,
    );
    remember_content(control);
    control
}

unsafe fn combo_add(hwnd: HWND, text: &str) {
    let text = win::to_wide(text);
    SendMessageW(hwnd, CB_ADDSTRING, 0, text.as_ptr() as LPARAM);
}

unsafe fn button(
    hwnd: HWND,
    id: i32,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    is_default: bool,
) {
    child(
        hwnd,
        "BUTTON",
        text,
        WS_CHILD
            | WS_VISIBLE
            | WS_TABSTOP
            | (if is_default {
                BS_DEFPUSHBUTTON
            } else {
                BS_PUSHBUTTON
            }) as WINDOW_STYLE,
        0,
        x,
        y,
        width,
        height,
        id,
    );
}

unsafe fn content_button(
    hwnd: HWND,
    id: i32,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    is_default: bool,
) {
    let control = child(
        hwnd,
        "BUTTON",
        text,
        WS_CHILD
            | WS_VISIBLE
            | WS_TABSTOP
            | (if is_default {
                BS_DEFPUSHBUTTON
            } else {
                BS_PUSHBUTTON
            }) as WINDOW_STYLE,
        0,
        x,
        y,
        width,
        height,
        id,
    );
    remember_content(control);
}

unsafe fn child(
    parent: HWND,
    class_name: &str,
    text: &str,
    style: WINDOW_STYLE,
    ex_style: WINDOW_EX_STYLE,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    id: i32,
) -> HWND {
    let class_name = win::to_wide(class_name);
    let text = win::to_wide(text);
    let control = CreateWindowExW(
        ex_style,
        class_name.as_ptr(),
        text.as_ptr(),
        style,
        x,
        y,
        width,
        height,
        parent,
        id as isize as HMENU,
        null_mut(),
        null_mut(),
    );

    if !control.is_null() {
        let font = GetStockObject(DEFAULT_GUI_FONT);
        SendMessageW(control, WM_SETFONT, font as WPARAM, 1);
    }

    control
}

unsafe fn control_text(hwnd: HWND, id: i32) -> String {
    let control = GetDlgItem(hwnd, id);
    if control.is_null() {
        return String::new();
    }
    let length = GetWindowTextLengthW(control);
    let mut buffer = vec![0u16; length as usize + 1];
    GetWindowTextW(control, buffer.as_mut_ptr(), buffer.len() as i32);
    String::from_utf16_lossy(&buffer[..length as usize])
}

unsafe fn set_control_text(hwnd: HWND, id: i32, text: &str) {
    let control = GetDlgItem(hwnd, id);
    if control.is_null() {
        return;
    }

    let text = win::to_wide(text);
    SetWindowTextW(control, text.as_ptr());
}

unsafe fn read_hex_color(hwnd: HWND, id: i32, label: &str) -> Option<ClickColor> {
    let text = control_text(hwnd, id);
    ClickColor::from_hex(&text).or_else(|| {
        error(hwnd, &format!("{label}은 #RRGGBB 형식으로 입력하세요."));
        None
    })
}

unsafe fn error(hwnd: HWND, message: &str) {
    let title = win::to_wide("설정 오류");
    let message = win::to_wide(message);
    MessageBoxW(hwnd, message.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR);
}

fn active_window() -> Option<HWND> {
    let value = SETTINGS_HWND.load(Ordering::Relaxed);
    if value == 0 {
        None
    } else {
        Some(value as HWND)
    }
}
