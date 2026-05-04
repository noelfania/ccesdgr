use std::fs::File;
use std::path::PathBuf;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::SIZE;
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
    GetDIBits, GetStockObject, GetTextExtentPoint32W, ReleaseDC, SelectObject, SetBkMode,
    SetTextColor, TextOutW, BITMAPINFO, BITMAPINFOHEADER, DEFAULT_GUI_FONT, DIB_RGB_COLORS, HDC,
    SRCCOPY,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DrawIcon, GetCursorInfo, CURSORINFO, CURSOR_SHOWING,
};

use crate::win;

static RECORDING: AtomicBool = AtomicBool::new(false);
static STOP_SIGNAL: AtomicBool = AtomicBool::new(false);
static LAST_OUTPUT: Mutex<PathBuf> = Mutex::new(PathBuf::new());

pub struct CaptureRegion {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub fn start(region: CaptureRegion, output_path: PathBuf, show_timestamp: bool) {
    if RECORDING.swap(true, Ordering::SeqCst) {
        return; // 이미 녹화 중
    }
    STOP_SIGNAL.store(false, Ordering::Relaxed);
    *LAST_OUTPUT.lock().expect("recorder mutex poisoned") = output_path.clone();

    thread::spawn(move || {
        record_loop(region, output_path, show_timestamp);
        RECORDING.store(false, Ordering::Relaxed);
    });
}

pub fn stop() {
    STOP_SIGNAL.store(true, Ordering::Relaxed);
}

pub fn is_recording() -> bool {
    RECORDING.load(Ordering::Relaxed)
}

pub fn last_output_path() -> PathBuf {
    LAST_OUTPUT.lock().expect("recorder mutex poisoned").clone()
}

fn record_loop(region: CaptureRegion, output_path: PathBuf, show_timestamp: bool) {
    let file = match File::create(&output_path) {
        Ok(f) => f,
        Err(_) => return,
    };

    let w = region.width.max(1) as u16;
    let h = region.height.max(1) as u16;

    let mut encoder = match gif::Encoder::new(file, w, h, &[]) {
        Ok(e) => e,
        Err(_) => return,
    };
    let _ = encoder.set_repeat(gif::Repeat::Infinite);

    let frame_interval = Duration::from_millis(100); // 10 FPS
    let recording_start = Instant::now();

    loop {
        if STOP_SIGNAL.load(Ordering::Relaxed) {
            break;
        }

        let frame_start = Instant::now();

        let timestamp = show_timestamp.then(|| format_elapsed(recording_start.elapsed()));
        if let Some(bgra) = capture_region(
            region.x,
            region.y,
            region.width,
            region.height,
            timestamp.as_deref(),
        ) {
            // GetDIBits는 BGRA 순서 → RGB로 변환
            let rgb: Vec<u8> = bgra
                .chunks_exact(4)
                .flat_map(|p| [p[2], p[1], p[0]])
                .collect();

            if rgb.len() == w as usize * h as usize * 3 {
                let mut frame = gif::Frame::from_rgb_speed(w, h, &rgb, 10);
                frame.delay = 10; // 10 × 10ms = 100ms
                if encoder.write_frame(&frame).is_err() {
                    break;
                }
            }
        }

        let elapsed = frame_start.elapsed();
        if elapsed < frame_interval {
            thread::sleep(frame_interval - elapsed);
        }
    }

    drop(encoder); // GIF 파일 완성 (flush)
}

fn capture_region(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    timestamp: Option<&str>,
) -> Option<Vec<u8>> {
    if width <= 0 || height <= 0 {
        return None;
    }

    unsafe {
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

        let old_obj = SelectObject(mem_dc, bitmap);
        BitBlt(mem_dc, 0, 0, width, height, screen_dc, x, y, SRCCOPY);

        // 마우스 커서 합성
        let mut ci: CURSORINFO = std::mem::zeroed();
        ci.cbSize = std::mem::size_of::<CURSORINFO>() as u32;
        if GetCursorInfo(&mut ci) != 0 && ci.flags & CURSOR_SHOWING != 0 {
            let cx = ci.ptScreenPos.x - x;
            let cy = ci.ptScreenPos.y - y;
            if cx >= 0 && cy >= 0 && cx < width && cy < height {
                DrawIcon(mem_dc, cx, cy, ci.hCursor);
            }
        }

        if let Some(text) = timestamp {
            draw_timestamp(mem_dc, width, height, text);
        }

        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = width;
        bmi.bmiHeader.biHeight = -height; // 음수 = top-down 순서
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = 0; // BI_RGB

        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let rows = GetDIBits(
            mem_dc,
            bitmap,
            0,
            height as u32,
            pixels.as_mut_ptr() as *mut _,
            &mut bmi,
            DIB_RGB_COLORS,
        );

        SelectObject(mem_dc, old_obj);
        DeleteObject(bitmap);
        DeleteDC(mem_dc);
        ReleaseDC(null_mut(), screen_dc);

        if rows == 0 { None } else { Some(pixels) }
    }
}

fn format_elapsed(elapsed: Duration) -> String {
    let total_deciseconds = elapsed.as_millis() / 100;
    let minutes = total_deciseconds / 600;
    let seconds = (total_deciseconds / 10) % 60;
    let tenths = total_deciseconds % 10;
    format!("{:02}:{:02}.{}", minutes, seconds, tenths)
}

unsafe fn draw_timestamp(hdc: HDC, width: i32, height: i32, text: &str) {
    if width <= 0 || height <= 0 {
        return;
    }

    let text_w = win::to_wide(text);
    let text_len = (text_w.len() - 1) as i32;
    let font = GetStockObject(DEFAULT_GUI_FONT);
    let old_font = SelectObject(hdc, font);
    SetBkMode(hdc, 1);

    let mut size = SIZE { cx: 0, cy: 0 };
    GetTextExtentPoint32W(hdc, text_w.as_ptr(), text_len, &mut size);

    let x = (width - size.cx - 8).max(0);
    let y = (height - size.cy - 8).max(0);

    // 검은 그림자를 먼저 찍어 밝은 배경에서도 읽히게 한다.
    SetTextColor(hdc, win::colorref(0, 0, 0));
    TextOutW(hdc, x + 1, y + 1, text_w.as_ptr(), text_len);
    SetTextColor(hdc, win::colorref(255, 255, 255));
    TextOutW(hdc, x, y, text_w.as_ptr(), text_len);

    SelectObject(hdc, old_font);
}
