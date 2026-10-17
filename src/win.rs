use std::ffi::OsStr;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{COLORREF, HINSTANCE, HWND, RECT};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetSaveFileNameW, OFN_EXPLORER, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_CXSCREEN, SM_CYSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

/// exe·트레이 공용 아이콘 (assets/icon-entry-3.png → build.rs ICO)
pub const APP_ICON_ID: usize = 1;

pub fn module_handle() -> HINSTANCE {
    unsafe { GetModuleHandleW(null()) }
}

pub fn to_wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}

pub fn colorref(red: u8, green: u8, blue: u8) -> COLORREF {
    red as COLORREF | ((green as COLORREF) << 8) | ((blue as COLORREF) << 16)
}

pub fn low_word(value: usize) -> u16 {
    (value & 0xffff) as u16
}

pub fn resource_id(id: usize) -> *const u16 {
    id as *const u16
}

pub fn virtual_screen_rect() -> RECT {
    unsafe {
        let left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let top = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        RECT {
            left,
            top,
            right: left + width,
            bottom: top + height,
        }
    }
}

pub fn primary_screen_rect() -> RECT {
    unsafe {
        let width = GetSystemMetrics(SM_CXSCREEN);
        let height = GetSystemMetrics(SM_CYSCREEN);
        RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        }
    }
}

pub fn rect_width(rect: &RECT) -> i32 {
    rect.right - rect.left
}

pub fn rect_height(rect: &RECT) -> i32 {
    rect.bottom - rect.top
}

pub fn intersect_rect(a: RECT, b: RECT) -> Option<RECT> {
    let left = a.left.max(b.left);
    let top = a.top.max(b.top);
    let right = a.right.min(b.right);
    let bottom = a.bottom.min(b.bottom);
    if right > left && bottom > top {
        Some(RECT {
            left,
            top,
            right,
            bottom,
        })
    } else {
        None
    }
}

/// rect를 가상 화면 안으로 잘라낸다. 교집합이 있으면 true.
pub fn clamp_rect_to_virtual_screen(rect: &mut RECT) -> bool {
    let screen = virtual_screen_rect();
    if let Some(inter) = intersect_rect(*rect, screen) {
        *rect = inter;
        true
    } else {
        false
    }
}

/// GIF 다른 이름으로 저장. Cancel이면 None.
pub unsafe fn save_gif_dialog(
    owner: HWND,
    initial_dir: &Path,
    default_name: &str,
) -> Option<PathBuf> {
    let mut file_buf = vec![0u16; 1024];
    let name_wide: Vec<u16> = OsStr::new(default_name)
        .encode_wide()
        .chain(Some(0))
        .collect();
    let copy_len = (name_wide.len() - 1).min(file_buf.len() - 1);
    file_buf[..copy_len].copy_from_slice(&name_wide[..copy_len]);

    // GIF\0*.gif\0\0
    let filter: Vec<u16> = "GIF\0*.gif\0\0".encode_utf16().collect();
    let title = to_wide("Save GIF");
    let def_ext = to_wide("gif");
    let initial = to_wide(&initial_dir.to_string_lossy());

    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        hInstance: null_mut(),
        lpstrFilter: filter.as_ptr(),
        lpstrCustomFilter: null_mut(),
        nMaxCustFilter: 0,
        nFilterIndex: 1,
        lpstrFile: file_buf.as_mut_ptr(),
        nMaxFile: file_buf.len() as u32,
        lpstrFileTitle: null_mut(),
        nMaxFileTitle: 0,
        lpstrInitialDir: initial.as_ptr(),
        lpstrTitle: title.as_ptr(),
        Flags: OFN_EXPLORER | OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST,
        nFileOffset: 0,
        nFileExtension: 0,
        lpstrDefExt: def_ext.as_ptr(),
        lCustData: 0,
        lpfnHook: None,
        lpTemplateName: null(),
        pvReserved: null_mut(),
        dwReserved: 0,
        FlagsEx: 0,
    };

    if GetSaveFileNameW(&mut ofn) == 0 {
        return None;
    }

    let end = file_buf.iter().position(|&c| c == 0).unwrap_or(file_buf.len());
    let path = std::ffi::OsString::from_wide(&file_buf[..end]);
    Some(PathBuf::from(path))
}

/// TOPMOST 창에서도 가려지지 않는 툴팁 컨트롤 생성
pub unsafe fn create_tooltip(owner: HWND) -> HWND {
    use windows_sys::Win32::UI::Controls::{TOOLTIPS_CLASSW, TTS_ALWAYSTIP, TTS_NOPREFIX};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, CW_USEDEFAULT, WS_EX_TOPMOST, WS_POPUP,
    };
    CreateWindowExW(
        WS_EX_TOPMOST,
        TOOLTIPS_CLASSW,
        null(),
        WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        owner,
        null_mut(),
        module_handle(),
        null_mut(),
    )
}

/// 컨트롤에 툴팁 문자열 연결 (TTF_SUBCLASS로 마우스 메시지 자동 전달)
pub unsafe fn add_tooltip(tooltip: HWND, owner: HWND, control: HWND, text: &str) {
    use windows_sys::Win32::UI::Controls::{TTF_IDISHWND, TTF_SUBCLASS, TTM_ADDTOOLW, TTTOOLINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;
    if tooltip.is_null() || control.is_null() {
        return;
    }
    let mut text = to_wide(text);
    let mut info: TTTOOLINFOW = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<TTTOOLINFOW>() as u32;
    info.uFlags = TTF_IDISHWND | TTF_SUBCLASS;
    info.hwnd = owner;
    info.uId = control as usize;
    info.lpszText = text.as_mut_ptr();
    SendMessageW(tooltip, TTM_ADDTOOLW, 0, &info as *const _ as isize);
}

/// 폴더 선택 대화상자. Cancel이면 None.
pub unsafe fn browse_folder_dialog(owner: HWND, title: &str) -> Option<PathBuf> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{
        SHBrowseForFolderW, SHGetPathFromIDListW, BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS,
        BROWSEINFOW,
    };
    let title = to_wide(title);
    let mut display = vec![0u16; 260];
    let mut info: BROWSEINFOW = std::mem::zeroed();
    info.hwndOwner = owner;
    info.pszDisplayName = display.as_mut_ptr();
    info.lpszTitle = title.as_ptr();
    info.ulFlags = BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE;

    let pidl = SHBrowseForFolderW(&info);
    if pidl.is_null() {
        return None;
    }
    let mut path = vec![0u16; 1024];
    let ok = SHGetPathFromIDListW(pidl, path.as_mut_ptr()) != 0;
    CoTaskMemFree(pidl as *const _);
    if !ok {
        return None;
    }
    let end = path.iter().position(|&c| c == 0).unwrap_or(path.len());
    Some(PathBuf::from(std::ffi::OsString::from_wide(&path[..end])))
}

/// 창이 놓인 모니터의 DPI (실패 시 96)
pub fn dpi_for_window(hwnd: HWND) -> u32 {
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 {
        96
    } else {
        dpi
    }
}

/// 96 DPI 기준 px 값을 현재 DPI로 변환
pub fn scale(value: i32, dpi: u32) -> i32 {
    (value as i64 * dpi as i64 / 96) as i32
}

/// DPI에 맞춘 Segoe UI 9pt 폰트 (호출자가 DeleteObject 책임)
pub unsafe fn create_ui_font(dpi: u32) -> windows_sys::Win32::Graphics::Gdi::HFONT {
    use windows_sys::Win32::Graphics::Gdi::CreateFontW;
    let face = to_wide("Segoe UI");
    let height = -((9 * dpi as i32 + 36) / 72);
    CreateFontW(
        height, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, // CLEARTYPE_QUALITY
        0,
        face.as_ptr(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_follows_dpi() {
        assert_eq!(scale(100, 96), 100);
        assert_eq!(scale(100, 144), 150);
        assert_eq!(scale(30, 192), 60);
    }

    #[test]
    fn intersect_and_clamp_rect() {
        let a = RECT { left: 0, top: 0, right: 100, bottom: 100 };
        let b = RECT { left: 50, top: 50, right: 150, bottom: 150 };
        let inter = intersect_rect(a, b).expect("overlap");
        assert_eq!((inter.left, inter.top, inter.right, inter.bottom), (50, 50, 100, 100));
        let far = RECT { left: 200, top: 200, right: 300, bottom: 300 };
        assert!(intersect_rect(a, far).is_none());
    }

    #[test]
    fn wide_strings_are_nul_terminated() {
        assert_eq!(to_wide("ab"), vec![b'a' as u16, b'b' as u16, 0]);
    }
}

/// 앱 아이콘을 시스템 DPI에 맞는 크기로 로드 (small = 트레이/제목줄용).
/// LoadIconW는 항상 32px(SM_CXICON)을 골라 트레이에서 축소·번지므로 LoadIconMetric 사용.
/// 프로세스 수명 동안 캐시한다.
pub fn app_icon(small: bool) -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
    use std::sync::atomic::{AtomicIsize, Ordering};
    use windows_sys::Win32::UI::Controls::{LoadIconMetric, LIM_LARGE, LIM_SMALL};
    use windows_sys::Win32::UI::WindowsAndMessaging::{LoadIconW, IDI_APPLICATION};

    static SMALL: AtomicIsize = AtomicIsize::new(0);
    static LARGE: AtomicIsize = AtomicIsize::new(0);
    let cache = if small { &SMALL } else { &LARGE };
    let cached = cache.load(Ordering::Relaxed);
    if cached != 0 {
        return cached as _;
    }

    let mut icon = null_mut();
    let metric = if small { LIM_SMALL } else { LIM_LARGE };
    let hr = unsafe { LoadIconMetric(module_handle(), resource_id(APP_ICON_ID), metric, &mut icon) };
    if hr < 0 || icon.is_null() {
        icon = unsafe { LoadIconW(module_handle(), resource_id(APP_ICON_ID)) };
    }
    if icon.is_null() {
        icon = unsafe { LoadIconW(null_mut(), IDI_APPLICATION) };
    }
    cache.store(icon as isize, Ordering::Relaxed);
    icon
}
