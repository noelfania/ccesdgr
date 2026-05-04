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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AppConfig {
    pub left_click_color: ClickColor,
    pub right_click_color: ClickColor,
    pub start_with_windows: bool,
    #[serde(default = "default_ripple_radius")]
    pub ripple_radius: i32,
    #[serde(default = "default_gif_output_path")]
    pub gif_output_path: String,
    #[serde(default)]
    pub open_folder_after_save: bool,
    #[serde(default)]
    pub recent_left_colors: Vec<ClickColor>,
    #[serde(default)]
    pub recent_right_colors: Vec<ClickColor>,
    #[serde(default = "default_ripple_lifetime")]
    pub ripple_lifetime: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            left_click_color: ClickColor::new(0, 150, 255),
            right_click_color: ClickColor::new(255, 96, 96),
            start_with_windows: false,
            ripple_radius: DEFAULT_RIPPLE_RADIUS,
            gif_output_path: default_gif_output_path(),
            open_folder_after_save: false,
            recent_left_colors: Vec::new(),
            recent_right_colors: Vec::new(),
            ripple_lifetime: DEFAULT_RIPPLE_LIFETIME,
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        let path = config_path();
        let Ok(contents) = fs::read_to_string(path) else {
            return Self::default();
        };

        let mut config: Self = serde_json::from_str(&contents).unwrap_or_default();
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
        fs::write(path, contents)
    }

    pub fn clamp_values(&mut self) {
        self.ripple_radius = self
            .ripple_radius
            .clamp(MIN_RIPPLE_RADIUS, MAX_RIPPLE_RADIUS);
        self.ripple_lifetime = self
            .ripple_lifetime
            .clamp(MIN_RIPPLE_LIFETIME, MAX_RIPPLE_LIFETIME);
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

pub fn config_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("LOCALAPPDATA"))
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    base.join("GMTL Mouse Click").join("config.json")
}
