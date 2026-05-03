use std::mem::size_of;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    GetStockObject, InvalidateRect, UpdateWindow, COLOR_3DFACE, DEFAULT_GUI_FONT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{
    CheckDlgButton, IsDlgButtonChecked, BST_CHECKED, BST_UNCHECKED,
};
use windows_sys::Win32::UI::Controls::Dialogs::{
    ChooseColorW, CHOOSECOLORW, CC_FULLOPEN, CC_RGBINIT, CC_SOLIDCOLOR,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowTextLengthW, GetWindowTextW,
    LoadCursorW, LoadIconW, MessageBoxW, RegisterClassExW, SendMessageW, ShowWindow, BS_AUTOCHECKBOX,
    BS_DEFPUSHBUTTON, BS_GROUPBOX, BS_PUSHBUTTON, CW_USEDEFAULT, ES_AUTOHSCROLL, HMENU,
    IDC_ARROW, MB_ICONERROR, MB_OK, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_COMMAND,
    WM_CREATE, WM_DESTROY, WM_SETFONT, WNDCLASSEXW, WS_BORDER, WS_CAPTION, WS_CHILD,
    WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

use crate::config::{AppConfig, ClickColor, MAX_RIPPLE_RADIUS, MIN_RIPPLE_RADIUS};
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
const ID_NAV_GENERAL: i32 = 2101;
const ID_NAV_MOUSE: i32 = 2102;
const ID_NAV_INFO: i32 = 2103;

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
            385,
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
                    choose_click_color(hwnd, true);
                    if *CURRENT_PAGE.lock().expect("settings page mutex poisoned") == PAGE_MOUSE {
                        show_page(hwnd, PAGE_MOUSE);
                    }
                    0
                }
                ID_RIGHT_COLOR => {
                    choose_click_color(hwnd, false);
                    if *CURRENT_PAGE.lock().expect("settings page mutex poisoned") == PAGE_MOUSE {
                        show_page(hwnd, PAGE_MOUSE);
                    }
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
        WM_DESTROY => {
            if SETTINGS_HWND.load(Ordering::Relaxed) == hwnd as isize {
                SETTINGS_HWND.store(0, Ordering::Relaxed);
            }
            let owner = windows_sys::Win32::UI::WindowsAndMessaging::GetWindow(hwnd, windows_sys::Win32::UI::WindowsAndMessaging::GW_OWNER);
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

    button(hwnd, ID_RESET, "초기화", 16, 304, 86, 28, false);
    button(hwnd, ID_CONFIRM, "확인", 320, 304, 78, 28, true);
    button(hwnd, ID_CANCEL, "취소", 406, 304, 78, 28, false);
    button(hwnd, ID_APPLY, "적용", 492, 304, 78, 28, false);

    show_page(hwnd, PAGE_GENERAL);
}

unsafe fn save_settings(hwnd: HWND) {
    if !apply_settings_from_window(hwnd) {
        return;
    }

    DestroyWindow(hwnd);
}

unsafe fn apply_settings_from_window(hwnd: HWND) -> bool {
    sync_current_page(hwnd);

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
    sync_current_page(hwnd);
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

unsafe fn sync_current_page(hwnd: HWND) {
    let page = *CURRENT_PAGE.lock().expect("settings page mutex poisoned");
    let has_content = !CONTENT_CONTROLS
        .lock()
        .expect("settings controls mutex poisoned")
        .is_empty();
    if !has_content {
        return;
    }

    if let Some(config) = CURRENT_CONFIG
        .lock()
        .expect("settings mutex poisoned")
        .as_mut()
    {
        match page {
            PAGE_GENERAL => {
                config.start_with_windows = IsDlgButtonChecked(hwnd, ID_STARTUP_CHECK) == BST_CHECKED;
            }
            PAGE_MOUSE => {
                if let Some(radius) = control_text(hwnd, ID_RIPPLE_RADIUS)
                    .trim()
                    .parse::<i32>()
                    .ok()
                {
                    config.ripple_radius = radius.clamp(MIN_RIPPLE_RADIUS, MAX_RIPPLE_RADIUS);
                }
            }
            _ => {}
        }
    }
}

unsafe fn create_general_page(hwnd: HWND) {
    content_group_box(hwnd, "기본 설정", 16, 52, 554, 218);
    content_label(hwnd, "윈도우 실행시 기동", 42, 92, 180, 24);
    content_label(hwnd, "체크하면 다음 Windows 로그인부터 자동으로 실행됩니다.", 42, 122, 360, 22);

    let checkbox = content_checkbox(hwnd, ID_STARTUP_CHECK, "", 512, 92, 40, 24);
    let checked = CURRENT_CONFIG
        .lock()
        .expect("settings mutex poisoned")
        .as_ref()
        .map(|config| config.start_with_windows)
        .unwrap_or_default();
    CheckDlgButton(
        hwnd,
        ID_STARTUP_CHECK,
        if checked { BST_CHECKED } else { BST_UNCHECKED },
    );
    remember_content(checkbox);
}

unsafe fn create_mouse_page(hwnd: HWND) {
    let config = CURRENT_CONFIG
        .lock()
        .expect("settings mutex poisoned")
        .clone()
        .unwrap_or_default();

    content_group_box(hwnd, "마우스 설정", 16, 52, 554, 218);

    content_label(hwnd, "왼쪽 클릭 효과 색상", 42, 92, 180, 24);
    content_label(hwnd, &config.left_click_color.to_hex(), 322, 92, 90, 24);
    content_button(hwnd, ID_LEFT_COLOR, "색상 선택", 442, 86, 100, 32, false);

    content_label(hwnd, "오른쪽 클릭 효과 색상", 42, 146, 180, 24);
    content_label(hwnd, &config.right_click_color.to_hex(), 322, 146, 90, 24);
    content_button(hwnd, ID_RIGHT_COLOR, "색상 선택", 442, 140, 100, 32, false);

    content_label(hwnd, "파장 반지름", 42, 200, 180, 24);
    content_edit(
        hwnd,
        ID_RIPPLE_RADIUS,
        &config.ripple_radius.to_string(),
        322,
        196,
        90,
        24,
    );
    content_label(
        hwnd,
        &format!("{} ~ {} px", MIN_RIPPLE_RADIUS, MAX_RIPPLE_RADIUS),
        430,
        200,
        120,
        24,
    );
}

unsafe fn create_info_page(hwnd: HWND) {
    content_group_box(hwnd, "프로그램 정보", 16, 52, 554, 218);
    content_label(hwnd, "GMTL Mouse Click", 42, 92, 220, 24);
    content_label(hwnd, "마우스 클릭 위치에 원형 파장 효과를 표시합니다.", 42, 126, 420, 24);
    content_label(hwnd, "트레이 아이콘에서 설정창을 다시 열거나 종료할 수 있습니다.", 42, 156, 460, 24);
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

unsafe fn choose_click_color(hwnd: HWND, is_left: bool) {
    let Some(mut config) = CURRENT_CONFIG
        .lock()
        .expect("settings mutex poisoned")
        .clone()
    else {
        return;
    };

    let initial_color = if is_left {
        config.left_click_color
    } else {
        config.right_click_color
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

    if is_left {
        config.left_click_color = ClickColor::from_colorref(dialog.rgbResult);
    } else {
        config.right_click_color = ClickColor::from_colorref(dialog.rgbResult);
    }

    *CURRENT_CONFIG.lock().expect("settings mutex poisoned") = Some(config);
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

unsafe fn content_edit(
    hwnd: HWND,
    id: i32,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) {
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
    let control = windows_sys::Win32::UI::WindowsAndMessaging::GetDlgItem(hwnd, id);
    if control.is_null() {
        return String::new();
    }

    let length = GetWindowTextLengthW(control);
    let mut buffer = vec![0u16; length as usize + 1];
    GetWindowTextW(control, buffer.as_mut_ptr(), buffer.len() as i32);
    String::from_utf16_lossy(&buffer[..length as usize])
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
