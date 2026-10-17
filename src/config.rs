use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use windows_sys::Win32::Foundation::COLORREF;

use crate::win;

pub const DEFAULT_RIPPLE_RADIUS: i32 = 72;
pub const MIN_RIPPLE_RADIUS: i32 = 20;
pub const MAX_RIPPLE_RADIUS: i32 = 240;
pub const DEFAULT_RIPPLE_LIFETIME: u32 = 28;
pub const MIN_RIPPLE_LIFETIME: u32 = 10;
pub const MAX_RIPPLE_LIFETIME: u32 = 60;
pub const DEFAULT_GIF_TIMESTAMP_FONT_SIZE: i32 = 14;
pub const MIN_GIF_TIMESTAMP_FONT_SIZE: i32 = 8;
pub const MAX_GIF_TIMESTAMP_FONT_SIZE: i32 = 24;

pub fn default_gif_output_path() -> String {
    std::env::var("USERPROFILE")
        .map(|p| format!("{}\\Downloads", p))
        .unwrap_or_else(|_| ".".to_string())
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct ClickColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl ClickColor {
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    pub fn to_colorref(self) -> COLORREF {
        win::colorref(self.red, self.green, self.blue)
    }

    pub fn from_colorref(value: COLORREF) -> Self {
        Self {
            red: (value & 0xff) as u8,
            green: ((value >> 8) & 0xff) as u8,
            blue: ((value >> 16) & 0xff) as u8,
        }
    }

    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.red, self.green, self.blue)
    }

    pub fn from_hex(value: &str) -> Option<Self> {
        let hex = value.trim().trim_start_matches('#');
        if hex.len() != 6 {
            return None;
        }

        let red = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let green = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let blue = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(Self::new(red, green, blue))
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CaptureBarPos {
    #[default]
    Top,
    Bottom,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CaptureFrameConfig {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    #[serde(default)]
    pub bar_pos: CaptureBarPos,
}

impl CaptureFrameConfig {
    pub fn to_rect(self) -> windows_sys::Win32::Foundation::RECT {
        windows_sys::Win32::Foundation::RECT {
            left: self.left,
            top: self.top,
            right: self.right,
            bottom: self.bottom,
        }
    }

    pub fn from_rect(rect: windows_sys::Win32::Foundation::RECT, bar_pos: CaptureBarPos) -> Self {
        Self {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
            bar_pos,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum ClickEffectKind {
    Ripple,
    FilledFade,
    FilledCrossFade,
    ArcFade,
    ArcDotFade,
    CrossFade,
    XCrossFade,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AppConfig {
    pub left_click_color: ClickColor,
    pub right_click_color: ClickColor,
    #[serde(default = "default_click_effect_enabled")]
    pub click_effect_enabled: bool,
    pub start_with_windows: bool,
    #[serde(default = "default_ripple_radius")]
    pub ripple_radius: i32,
    #[serde(default = "default_gif_output_path")]
    pub gif_output_path: String,
    #[serde(default)]
    pub open_folder_after_save: bool,
    /// 녹화 종료 후 저장 위치/파일명 대화상자
    #[serde(default)]
    pub ask_gif_save_path: bool,
    #[serde(default)]
    pub show_gif_timestamp: bool,
    #[serde(default = "default_gif_timestamp_font_size")]
    pub gif_timestamp_font_size: i32,
    #[serde(default = "default_gif_even_size")]
    pub gif_even_size: bool,
    #[serde(default)]
    pub recent_left_colors: Vec<ClickColor>,
    #[serde(default)]
    pub recent_right_colors: Vec<ClickColor>,
    #[serde(default = "default_ripple_lifetime")]
    pub ripple_lifetime: u32,
    #[serde(default = "default_click_effect_kind")]
    pub click_effect_kind: ClickEffectKind,
    #[serde(default)]
    pub capture_frame: Option<CaptureFrameConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            left_click_color: ClickColor::new(0, 150, 255),
            right_click_color: ClickColor::new(255, 96, 96),
            click_effect_enabled: true,
            start_with_windows: false,
            ripple_radius: DEFAULT_RIPPLE_RADIUS,
            gif_output_path: default_gif_output_path(),
            open_folder_after_save: false,
            ask_gif_save_path: false,
            show_gif_timestamp: false,
            gif_timestamp_font_size: DEFAULT_GIF_TIMESTAMP_FONT_SIZE,
            gif_even_size: true,
            recent_left_colors: Vec::new(),
            recent_right_colors: Vec::new(),
            ripple_lifetime: DEFAULT_RIPPLE_LIFETIME,
            click_effect_kind: ClickEffectKind::Ripple,
            capture_frame: None,
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        let path = config_path();
        let Ok(contents) = fs::read_to_string(&path) else {
            return Self::default();
        };

        let mut config: Self = match serde_json::from_str(&contents) {
            Ok(config) => config,
            Err(_) => {
                // 손상된 설정이 다음 저장 때 덮어써지지 않도록 백업해 둔다.
                let _ = fs::copy(&path, path.with_extension("json.bak"));
                Self::default()
            }
        };
        config.clamp_values();
        config
    }

    pub fn save(&self) -> io::Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut config = self.clone();
        config.clamp_values();
        let contents = serde_json::to_string_pretty(&config)?;
        // 임시 파일에 쓴 뒤 교체해 저장 도중 중단돼도 기존 설정이 깨지지 않게 한다.
        let temp_path = path.with_extension("json.tmp");
        fs::write(&temp_path, contents)?;
        fs::rename(&temp_path, &path)
    }

    pub fn clamp_values(&mut self) {
        self.ripple_radius = self
            .ripple_radius
            .clamp(MIN_RIPPLE_RADIUS, MAX_RIPPLE_RADIUS);
        self.ripple_lifetime = self
            .ripple_lifetime
            .clamp(MIN_RIPPLE_LIFETIME, MAX_RIPPLE_LIFETIME);
        self.gif_timestamp_font_size = self
            .gif_timestamp_font_size
            .clamp(MIN_GIF_TIMESTAMP_FONT_SIZE, MAX_GIF_TIMESTAMP_FONT_SIZE);
        // 최근 색상 목록 최대 3개로 제한
        self.recent_left_colors.truncate(3);
        self.recent_right_colors.truncate(3);
    }
}

fn default_ripple_radius() -> i32 {
    DEFAULT_RIPPLE_RADIUS
}

fn default_ripple_lifetime() -> u32 {
    DEFAULT_RIPPLE_LIFETIME
}

fn default_gif_timestamp_font_size() -> i32 {
    DEFAULT_GIF_TIMESTAMP_FONT_SIZE
}

fn default_gif_even_size() -> bool {
    true
}

fn default_click_effect_enabled() -> bool {
    true
}

fn default_click_effect_kind() -> ClickEffectKind {
    ClickEffectKind::Ripple
}

pub fn config_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("LOCALAPPDATA"))
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    base.join("GMTL Mouse Click").join("config.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let color = ClickColor::new(0x12, 0xAB, 0xFF);
        assert_eq!(color.to_hex(), "#12ABFF");
        let parsed = ClickColor::from_hex(" #12abff ").expect("valid hex");
        assert_eq!((parsed.red, parsed.green, parsed.blue), (0x12, 0xAB, 0xFF));
    }

    #[test]
    fn invalid_hex_is_rejected() {
        assert!(ClickColor::from_hex("#12AB").is_none());
        assert!(ClickColor::from_hex("#GGGGGG").is_none());
        assert!(ClickColor::from_hex("").is_none());
    }

    #[test]
    fn colorref_round_trip() {
        let color = ClickColor::new(1, 2, 3);
        let back = ClickColor::from_colorref(color.to_colorref());
        assert_eq!((back.red, back.green, back.blue), (1, 2, 3));
    }

    #[test]
    fn clamp_values_limits_ranges_and_recent_colors() {
        let mut config = AppConfig {
            ripple_radius: 9999,
            ripple_lifetime: 0,
            gif_timestamp_font_size: 100,
            recent_left_colors: vec![ClickColor::new(0, 0, 0); 5],
            ..AppConfig::default()
        };
        config.clamp_values();
        assert_eq!(config.ripple_radius, MAX_RIPPLE_RADIUS);
        assert_eq!(config.ripple_lifetime, MIN_RIPPLE_LIFETIME);
        assert_eq!(config.gif_timestamp_font_size, MAX_GIF_TIMESTAMP_FONT_SIZE);
        assert_eq!(config.recent_left_colors.len(), 3);
    }

    #[test]
    fn missing_fields_use_defaults() {
        // 구버전 config.json (필수 필드만) 도 읽을 수 있어야 한다.
        let json = r#"{
            "left_click_color": {"red": 1, "green": 2, "blue": 3},
            "right_click_color": {"red": 4, "green": 5, "blue": 6},
            "start_with_windows": false
        }"#;
        let config: AppConfig = serde_json::from_str(json).expect("old config parses");
        assert!(config.click_effect_enabled);
        assert_eq!(config.ripple_radius, DEFAULT_RIPPLE_RADIUS);
        assert_eq!(config.click_effect_kind, ClickEffectKind::Ripple);
        assert!(config.capture_frame.is_none());
    }
}
