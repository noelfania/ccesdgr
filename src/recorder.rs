use std::fs::File;
use std::path::PathBuf;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::SIZE;
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, DeleteDC, DeleteObject, GetDC,
    GetDIBits, GetStockObject, GetTextExtentPoint32W, ReleaseDC, SelectObject, SetBkMode,
    SetTextColor, TextOutW, BITMAPINFO, BITMAPINFOHEADER, DEFAULT_GUI_FONT, DIB_RGB_COLORS, HBITMAP,
    HDC, HGDIOBJ, SRCCOPY,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DrawIcon, GetCursorInfo, CURSORINFO, CURSOR_SHOWING,
};

use crate::config::{MAX_GIF_TIMESTAMP_FONT_SIZE, MIN_GIF_TIMESTAMP_FONT_SIZE};
use crate::win;

const FW_NORMAL: i32 = 400;
const DEFAULT_CHARSET: u32 = 1;
const OUT_DEFAULT_PRECIS: u32 = 0;
const CLIP_DEFAULT_PRECIS: u32 = 0;
const DEFAULT_QUALITY: u32 = 0;
const FIXED_PITCH: u32 = 1;
const FF_MODERN: u32 = 0x30;

static RECORDING: AtomicBool = AtomicBool::new(false);
static STOP_SIGNAL: AtomicBool = AtomicBool::new(false);
static LAST_FRAME_COUNT: AtomicUsize = AtomicUsize::new(0);
static LAST_OUTPUT: Mutex<PathBuf> = Mutex::new(PathBuf::new());
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);
static STARTED_AT: Mutex<Option<Instant>> = Mutex::new(None);
static WORKER: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

pub struct CaptureRegion {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub fn start(
    region: CaptureRegion,
    output_path: PathBuf,
    show_timestamp: bool,
    timestamp_font_size: i32,
) {
    if RECORDING.swap(true, Ordering::SeqCst) {
        return; // 이미 녹화 중
    }
    STOP_SIGNAL.store(false, Ordering::Relaxed);
    LAST_FRAME_COUNT.store(0, Ordering::Relaxed);
    *LAST_OUTPUT.lock().expect("recorder mutex poisoned") = output_path.clone();
    *LAST_ERROR.lock().expect("recorder mutex poisoned") = None;
    *STARTED_AT.lock().expect("recorder mutex poisoned") = Some(Instant::now());

    let handle = thread::spawn(move || {
        let frame_count =
            match record_loop(region, output_path, show_timestamp, timestamp_font_size) {
                Ok(count) => count,
                Err(message) => {
                    *LAST_ERROR.lock().expect("recorder mutex poisoned") = Some(message);
                    0
                }
            };
        LAST_FRAME_COUNT.store(frame_count, Ordering::Relaxed);
        RECORDING.store(false, Ordering::Relaxed);
    });
    *WORKER.lock().expect("recorder mutex poisoned") = Some(handle);
}

pub fn stop() {
    STOP_SIGNAL.store(true, Ordering::Relaxed);
}

/// 종료 시 GIF 트레일러가 기록되도록 녹화 스레드 종료를 최대 `timeout`까지 기다린다.
pub fn stop_and_wait(timeout: Duration) {
    stop();
    let deadline = Instant::now() + timeout;
    while is_recording() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    if !is_recording() {
        if let Some(handle) = WORKER.lock().expect("recorder mutex poisoned").take() {
            let _ = handle.join();
        }
    }
}

/// 녹화 시작 후 경과 시간 (녹화 중일 때만)
pub fn elapsed() -> Option<Duration> {
    if !is_recording() {
        return None;
    }
    STARTED_AT
        .lock()
        .expect("recorder mutex poisoned")
        .map(|start| start.elapsed())
}

/// 마지막 녹화가 실패했다면 오류 메시지를 꺼낸다.
pub fn take_last_error() -> Option<String> {
    LAST_ERROR.lock().expect("recorder mutex poisoned").take()
}

pub fn is_recording() -> bool {
    RECORDING.load(Ordering::Relaxed)
}

pub fn last_output_path() -> PathBuf {
    LAST_OUTPUT.lock().expect("recorder mutex poisoned").clone()
}

pub fn set_last_output_path(path: PathBuf) {
    *LAST_OUTPUT.lock().expect("recorder mutex poisoned") = path;
}

pub fn last_frame_count() -> usize {
    LAST_FRAME_COUNT.load(Ordering::Relaxed)
}

/// 인코더가 밀릴 때 쌓아 둘 최대 프레임 수 (메모리 상한). 가득 차면 캡처가 기다리며,
/// 프레임 지연은 실제 경과 시간으로 계산하므로 재생 속도는 유지된다.
const ENCODE_QUEUE_LEN: usize = 8;

struct EncodeJob {
    bgra: Vec<u8>,
    delay: u16,
}

fn record_loop(
    region: CaptureRegion,
    output_path: PathBuf,
    show_timestamp: bool,
    timestamp_font_size: i32,
) -> Result<usize, String> {
    let file = File::create(&output_path)
        .map_err(|error| format!("Failed to create {}: {error}", output_path.display()))?;

    let w = region.width.max(1) as u16;
    let h = region.height.max(1) as u16;

    let mut encoder = gif::Encoder::new(file, w, h, &[])
        .map_err(|error| format!("Failed to start the GIF encoder: {error}"))?;
    let _ = encoder.set_repeat(gif::Repeat::Infinite);

    let timestamp_font_size =
        timestamp_font_size.clamp(MIN_GIF_TIMESTAMP_FONT_SIZE, MAX_GIF_TIMESTAMP_FONT_SIZE);
    let mut capturer = Capturer::new(
        region.width,
        region.height,
        show_timestamp.then_some(timestamp_font_size),
    )
    .ok_or_else(|| "Failed to prepare screen capture resources.".to_string())?;

    // 색 양자화(NeuQuant)가 무거우므로 인코딩은 별도 스레드에서 처리
    let (sender, receiver) = mpsc::sync_channel::<EncodeJob>(ENCODE_QUEUE_LEN);
    let encode_thread = thread::spawn(move || -> Result<usize, String> {
        let mut frame_count = 0usize;
        for job in receiver {
            let rgb = bgra_to_rgb(&job.bgra);
            write_gif_frame(&mut encoder, w, h, &rgb, job.delay)
                .map_err(|error| format!("Failed to write a GIF frame: {error}"))?;
            frame_count += 1;
        }
        drop(encoder); // GIF 파일 완성 (flush)
        Ok(frame_count)
    });

    let frame_interval = Duration::from_millis(100); // 10 FPS
    let default_delay = frame_delay_from_duration(frame_interval);
    let recording_start = Instant::now();
    let expected_len = w as usize * h as usize * 4;
    let mut pending_frame: Option<(Vec<u8>, Instant)> = None;

    loop {
        if STOP_SIGNAL.load(Ordering::Relaxed) {
            break;
        }

        let frame_start = Instant::now();
        let timestamp = show_timestamp.then(|| format_elapsed(recording_start.elapsed()));
        if let Some(bgra) = capturer.capture(region.x, region.y, timestamp.as_deref()) {
            if bgra.len() == expected_len {
                // 다음 프레임이 와야 이전 프레임의 표시 시간이 정해진다.
                if let Some((previous, previous_time)) =
                    pending_frame.replace((bgra.to_vec(), frame_start))
                {
                    let delay = frame_delay_from_duration(
                        frame_start.saturating_duration_since(previous_time),
                    );
                    if sender
                        .send(EncodeJob {
                            bgra: previous,
                            delay,
                        })
                        .is_err()
                    {
                        // 인코더 스레드가 오류로 종료됨 — 결과는 join에서 받는다.
                        pending_frame = None;
                        break;
                    }
                }
            }
        }

        let elapsed = frame_start.elapsed();
        if elapsed < frame_interval {
            thread::sleep(frame_interval - elapsed);
        }
    }

    if let Some((bgra, frame_time)) = pending_frame {
        let elapsed = Instant::now().saturating_duration_since(frame_time);
        let delay = if elapsed.is_zero() {
            default_delay
        } else {
            frame_delay_from_duration(elapsed)
        };
        let _ = sender.send(EncodeJob { bgra, delay });
    }
    drop(sender);
    drop(capturer);

    encode_thread
        .join()
        .map_err(|_| "The GIF encoder stopped unexpectedly.".to_string())?
}

fn bgra_to_rgb(bgra: &[u8]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(bgra.len() / 4 * 3);
    for pixel in bgra.chunks_exact(4) {
        rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
    }
    rgb
}

fn frame_delay_from_duration(duration: Duration) -> u16 {
    let centiseconds = ((duration.as_millis() + 5) / 10).clamp(1, u16::MAX as u128);
    centiseconds as u16
}

fn write_gif_frame<W: std::io::Write>(
    encoder: &mut gif::Encoder<W>,
    width: u16,
    height: u16,
    rgb: &[u8],
    delay: u16,
) -> Result<(), gif::EncodingError> {
    let mut frame = gif::Frame::from_rgb_speed(width, height, rgb, 10);
    frame.delay = delay;
    encoder.write_frame(&frame)
}

/// 녹화 동안 재사용하는 GDI 자원 (매 프레임 DC·비트맵·폰트를 만들지 않는다)
struct Capturer {
    screen_dc: HDC,
    mem_dc: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
    font: HGDIOBJ,
    owns_font: bool,
    width: i32,
    height: i32,
    pixels: Vec<u8>,
}

impl Capturer {
    fn new(width: i32, height: i32, timestamp_font_size: Option<i32>) -> Option<Self> {
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
            let old_bitmap = SelectObject(mem_dc, bitmap);

            let (font, owns_font) = match timestamp_font_size {
                Some(size) => {
                    let created = create_timestamp_font(size);
                    if created.is_null() {
                        (GetStockObject(DEFAULT_GUI_FONT), false)
                    } else {
                        (created, true)
                    }
                }
                None => (null_mut(), false),
            };

            Some(Self {
                screen_dc,
                mem_dc,
                bitmap,
                old_bitmap,
                font,
                owns_font,
                width,
                height,
                pixels: vec![0u8; width as usize * height as usize * 4],
            })
        }
    }

    /// 화면 영역 + 커서 + (선택) 타임스탬프를 BGRA(top-down)로 돌려준다.
    fn capture(&mut self, x: i32, y: i32, timestamp: Option<&str>) -> Option<&[u8]> {
        unsafe {
            if BitBlt(
                self.mem_dc,
                0,
                0,
                self.width,
                self.height,
                self.screen_dc,
                x,
                y,
                SRCCOPY,
            ) == 0
            {
                return None;
            }

            // 마우스 커서 합성
            let mut ci: CURSORINFO = std::mem::zeroed();
            ci.cbSize = std::mem::size_of::<CURSORINFO>() as u32;
            if GetCursorInfo(&mut ci) != 0 && ci.flags & CURSOR_SHOWING != 0 {
                let cx = ci.ptScreenPos.x - x;
                let cy = ci.ptScreenPos.y - y;
                if cx >= 0 && cy >= 0 && cx < self.width && cy < self.height {
                    DrawIcon(self.mem_dc, cx, cy, ci.hCursor);
                }
            }

            if let Some(text) = timestamp {
                if !self.font.is_null() {
                    draw_timestamp(self.mem_dc, self.font, self.width, self.height, text);
                }
            }

            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = self.width;
            bmi.bmiHeader.biHeight = -self.height; // 음수 = top-down 순서
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = 0; // BI_RGB

            // GetDIBits는 선택되지 않은 비트맵을 요구하므로 잠시 해제
            SelectObject(self.mem_dc, self.old_bitmap);
            let rows = GetDIBits(
                self.mem_dc,
                self.bitmap,
                0,
                self.height as u32,
                self.pixels.as_mut_ptr() as *mut _,
                &mut bmi,
                DIB_RGB_COLORS,
            );
            SelectObject(self.mem_dc, self.bitmap);

            if rows == 0 {
                None
            } else {
                Some(&self.pixels)
            }
        }
    }
}

impl Drop for Capturer {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.mem_dc, self.old_bitmap);
            DeleteObject(self.bitmap);
            if self.owns_font {
                DeleteObject(self.font);
            }
            DeleteDC(self.mem_dc);
            ReleaseDC(null_mut(), self.screen_dc);
        }
    }
}

pub fn format_elapsed(elapsed: Duration) -> String {
    let total_deciseconds = elapsed.as_millis() / 100;
    let minutes = total_deciseconds / 600;
    let seconds = (total_deciseconds / 10) % 60;
    let tenths = total_deciseconds % 10;
    format!("{:02}:{:02}.{}", minutes, seconds, tenths)
}

unsafe fn create_timestamp_font(font_size: i32) -> HGDIOBJ {
    let face = win::to_wide("Terminal");
    let font_size = font_size.clamp(MIN_GIF_TIMESTAMP_FONT_SIZE, MAX_GIF_TIMESTAMP_FONT_SIZE);
    CreateFontW(
        -font_size,
        0,
        0,
        0,
        FW_NORMAL,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        DEFAULT_QUALITY,
        FIXED_PITCH | FF_MODERN,
        face.as_ptr(),
    )
}

unsafe fn draw_timestamp(hdc: HDC, font: HGDIOBJ, width: i32, height: i32, text: &str) {
    if width <= 0 || height <= 0 {
        return;
    }

    let text_w = win::to_wide(text);
    let text_len = (text_w.len() - 1) as i32;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_delay_rounds_to_centiseconds() {
        assert_eq!(frame_delay_from_duration(Duration::from_millis(100)), 10);
        assert_eq!(frame_delay_from_duration(Duration::from_millis(104)), 10);
        assert_eq!(frame_delay_from_duration(Duration::from_millis(105)), 11);
        // 0ms라도 최소 1cs (0이면 뷰어가 기본 지연으로 바꿔 버린다)
        assert_eq!(frame_delay_from_duration(Duration::ZERO), 1);
        assert_eq!(
            frame_delay_from_duration(Duration::from_secs(10_000)),
            u16::MAX
        );
    }

    #[test]
    fn elapsed_is_formatted_as_minutes_seconds_tenths() {
        assert_eq!(format_elapsed(Duration::from_millis(0)), "00:00.0");
        assert_eq!(format_elapsed(Duration::from_millis(12_345)), "00:12.3");
        assert_eq!(format_elapsed(Duration::from_millis(61_999)), "01:01.9");
    }

    /// 실제 화면을 1초간 녹화한다 (데스크톱 세션 필요): `cargo test -- --ignored`
    #[test]
    #[ignore]
    fn records_a_short_gif_from_the_screen() {
        let path = std::env::temp_dir().join("ccesdgr-recorder-test.gif");
        let region = CaptureRegion {
            x: 0,
            y: 0,
            width: 320,
            height: 200,
        };
        start(region, path.clone(), true, 14);
        thread::sleep(Duration::from_millis(1000));
        stop_and_wait(Duration::from_secs(5));

        assert!(!is_recording());
        assert_eq!(take_last_error(), None);
        let frames = last_frame_count();
        assert!(frames >= 5, "expected ~10 frames, got {frames}");

        let bytes = std::fs::read(&path).expect("gif written");
        assert!(bytes.starts_with(b"GIF89a"));
        assert_eq!(bytes.last(), Some(&0x3B), "GIF trailer must be written");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn bgra_is_converted_to_rgb() {
        let bgra = [1, 2, 3, 255, 10, 20, 30, 0];
        assert_eq!(bgra_to_rgb(&bgra), vec![3, 2, 1, 30, 20, 10]);
    }
}
