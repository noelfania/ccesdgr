use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use std::thread;
use std::time::Duration;

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HBITMAP, SRCCOPY,
};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

use crate::select_rect;
use crate::win;

const CF_BITMAP_FORMAT: u32 = 2;
/// 다른 앱이 클립보드를 잠깐 점유해도 실패하지 않도록 재시도
const CLIPBOARD_RETRIES: u32 = 10;
const CLIPBOARD_RETRY_DELAY: Duration = Duration::from_millis(30);

pub fn run(owner: HWND) {
    let Some(rect) = select_rect::select_region(owner) else {
        return;
    };
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;

    let config = crate::current_config();
    let save_png = config.as_ref().is_some_and(|config| config.snapshot_save_png);

    let Some((bitmap, pixels)) = (unsafe { capture_region(rect, save_png) }) else {
        show_error(owner, "Failed to capture the selected area.");
        return;
    };

    // 클립보드가 실패해도 파일은 남도록 PNG를 먼저 저장한다.
    let saved = match (&config, pixels) {
        (Some(config), Some(pixels)) if save_png => Some(save_png_file(
            Path::new(&config.snapshot_output_path),
            &pixels,
            width,
            height,
        )),
        _ => None,
    };
    let copied = unsafe { set_clipboard_bitmap(owner, bitmap) }.is_ok();

    match (copied, saved) {
        (true, None) => {
            crate::tray::notify(&format!(
                "Snapshot copied to clipboard ({width}×{height})"
            ));
        }
        (true, Some(Ok(path))) => {
            crate::tray::notify(&format!(
                "Snapshot copied ({width}×{height}) and saved: {}",
                file_name(&path)
            ));
        }
        (true, Some(Err(message))) => {
            crate::tray::notify_error(&format!(
                "Snapshot copied, but saving the PNG failed: {message}"
            ));
        }
        (false, Some(Ok(path))) => {
            crate::tray::notify_error(&format!(
                "Clipboard unavailable. Snapshot saved: {}",
                file_name(&path)
            ));
        }
        (false, Some(Err(message))) => {
            show_error(
                owner,
                &format!("Failed to copy the snapshot to the clipboard.\n{message}"),
            );
        }
        (false, None) => {
            show_error(owner, "Failed to copy the snapshot to the clipboard.");
        }
    }
}

fn show_error(owner: HWND, message: &str) {
    let title = win::to_wide("GMTL Mouse Click");
    let message = win::to_wide(message);
    unsafe {
        MessageBoxW(owner, message.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR);
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// 화면 영역을 비트맵으로 복사한다. `want_pixels`면 PNG용 BGRA(top-down)도 함께 돌려준다.
unsafe fn capture_region(rect: RECT, want_pixels: bool) -> Option<(HBITMAP, Option<Vec<u8>>)> {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width <= 0 || height <= 0 {
        return None;
    }

    let screen_dc = GetDC(null_mut());
    if screen_dc.is_null() {
        return None;
    }

    let mem_dc = CreateCompatibleDC(screen_dc);
    if mem_dc.is_null() {
        ReleaseDC(null_mut(), screen_dc);
        return None;
    }

    let bitmap = CreateCompatibleBitmap(screen_dc, width, height);
    if bitmap.is_null() {
        DeleteDC(mem_dc);
        ReleaseDC(null_mut(), screen_dc);
        return None;
    }

    let old = SelectObject(mem_dc, bitmap);
    let copied = BitBlt(mem_dc, 0, 0, width, height, screen_dc, rect.left, rect.top, SRCCOPY) != 0;
    SelectObject(mem_dc, old);

    let pixels = if copied && want_pixels {
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = width;
        bmi.bmiHeader.biHeight = -height; // 음수 = top-down 순서
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = 0; // BI_RGB
        let rows = GetDIBits(
            mem_dc,
            bitmap,
            0,
            height as u32,
            pixels.as_mut_ptr() as *mut _,
            &mut bmi,
            DIB_RGB_COLORS,
        );
        (rows != 0).then_some(pixels)
    } else {
        None
    };

    DeleteDC(mem_dc);
    ReleaseDC(null_mut(), screen_dc);
    if !copied {
        DeleteObject(bitmap);
        return None;
    }

    Some((bitmap, pixels))
}

/// 비트맵을 클립보드에 넣는다. 성공 시 핸들 소유권은 클립보드로, 실패 시 여기서 해제.
pub unsafe fn set_clipboard_bitmap(owner: HWND, bitmap: HBITMAP) -> Result<(), ()> {
    let mut opened = OpenClipboard(owner) != 0;
    for _ in 0..CLIPBOARD_RETRIES {
        if opened {
            break;
        }
        thread::sleep(CLIPBOARD_RETRY_DELAY);
        opened = OpenClipboard(owner) != 0;
    }
    if !opened {
        DeleteObject(bitmap);
        return Err(());
    }
    EmptyClipboard();
    let handle = SetClipboardData(CF_BITMAP_FORMAT, bitmap);
    CloseClipboard();

    if handle.is_null() {
        DeleteObject(bitmap);
        Err(())
    } else {
        Ok(())
    }
}

/// BGRA(top-down) 픽셀을 폴더에 `snap-YYYYMMDD-HHMMSS.png`로 저장한다.
pub fn save_png_file(
    folder: &Path,
    bgra: &[u8],
    width: i32,
    height: i32,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(folder)
        .map_err(|error| format!("Failed to create {}: {error}", folder.display()))?;
    let path = next_snapshot_path(folder, &timestamp(), |candidate| candidate.exists());
    write_png(&path, bgra, width, height)?;
    Ok(path)
}

fn timestamp() -> String {
    let st = unsafe {
        let mut st: windows_sys::Win32::Foundation::SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&mut st);
        st
    };
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
    )
}

/// 같은 초에 겹치면 `-2`, `-3`… 을 붙여 덮어쓰지 않는다.
fn next_snapshot_path(folder: &Path, stamp: &str, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let first = folder.join(format!("snap-{stamp}.png"));
    if !exists(&first) {
        return first;
    }
    (2u32..)
        .map(|index| folder.join(format!("snap-{stamp}-{index}.png")))
        .find(|candidate| !exists(candidate))
        .expect("snapshot file name range exhausted")
}

fn write_png(path: &Path, bgra: &[u8], width: i32, height: i32) -> Result<(), String> {
    let describe = |error: &dyn std::fmt::Display| format!("{}: {error}", path.display());
    let file = File::create(path).map_err(|error| describe(&error))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width as u32, height as u32);
    // 화면 비트맵의 알파는 의미가 없어 RGB로 저장한다.
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let result = encoder.write_header().and_then(|mut writer| {
        writer.write_image_data(&crate::recorder::bgra_to_rgb(bgra))?;
        writer.finish()
    });
    if let Err(error) = result {
        // 쓰다 만 파일을 남기지 않는다.
        let _ = std::fs::remove_file(path);
        return Err(describe(&error));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_name_gets_a_suffix_when_taken() {
        let folder = Path::new("shots");
        let free = next_snapshot_path(folder, "20260101-000000", |_| false);
        assert_eq!(free, folder.join("snap-20260101-000000.png"));

        let taken = [
            folder.join("snap-20260101-000000.png"),
            folder.join("snap-20260101-000000-2.png"),
        ];
        let next = next_snapshot_path(folder, "20260101-000000", |path| {
            taken.iter().any(|item| item == path)
        });
        assert_eq!(next, folder.join("snap-20260101-000000-3.png"));
    }

    #[test]
    fn png_is_written_with_rgb_pixels() {
        let folder = std::env::temp_dir().join(format!("ccesdgr-png-{}", std::process::id()));
        // BGRA: 파랑, 빨강
        let bgra = [255, 0, 0, 0, 0, 0, 255, 0];
        let path = save_png_file(&folder, &bgra, 2, 1).expect("png saved");

        let decoder =
            png::Decoder::new(std::io::BufReader::new(File::open(&path).expect("png opens")));
        let mut reader = decoder.read_info().expect("png header");
        let mut decoded = vec![0u8; reader.output_buffer_size().expect("png size")];
        reader.next_frame(&mut decoded).expect("png frame");
        assert_eq!(&decoded[..6], &[0, 0, 255, 255, 0, 0]);

        let _ = std::fs::remove_dir_all(&folder);
    }
}
