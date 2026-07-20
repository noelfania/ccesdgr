---
status: active
audience: developer
verified: 2026-07-04
---

# Config and storage

Facts only — no procedures.

## config.json

| Item | Value |
|------|--------|
| Primary path | `%APPDATA%\GMTL Mouse Click\config.json` |
| Fallback | `%LOCALAPPDATA%\GMTL Mouse Click\config.json`, then cwd |
| Schema | `AppConfig` in [`src/config.rs`](../../src/config.rs) |

### AppConfig fields (summary)

| Field | Notes |
|-------|--------|
| `left_click_color` / `right_click_color` | RGB |
| `click_effect_enabled` | default true |
| `start_with_windows` | default false |
| `ripple_radius` | 20–240, default 72 |
| `ripple_lifetime` | 10–60, default 28 |
| `click_effect_kind` | enum, default Ripple |
| `gif_output_path` | default `%USERPROFILE%\Downloads` |
| `ask_gif_save_path` | default false — prompt Save As after recording |
| `open_folder_after_save` | default false |
| `show_gif_timestamp` | default false |
| `gif_timestamp_font_size` | 8–24, default 14 |
| `gif_even_size` | default true |
| `recent_left_colors` / `recent_right_colors` | max 3 each |
| `capture_frame` | optional; last GIF frame rect + `bar_pos` (`top` \| `bottom`) |

### capture_frame example

```json
"capture_frame": {
  "left": 100,
  "top": 80,
  "right": 740,
  "bottom": 560,
  "bar_pos": "top"
}
```

Restored on next GIF UI open; invalid/off-screen saved rect falls back to primary monitor center (640×480).

## Windows startup (registry)

| Item | Value |
|------|--------|
| Hive | `HKEY_CURRENT_USER` |
| Key | `Software\Microsoft\Windows\CurrentVersion\Run` |
| Value name | `GMTL Mouse Click` |
| Value | Quoted path to current exe |

Implementation: [`src/startup.rs`](../../src/startup.rs)

## Window class names

See [architecture.md](architecture.md) §10.
