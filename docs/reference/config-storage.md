---
status: active
audience: developer
verified: 2026-07-25
---

# Config and storage

Facts only — no procedures. Schema SSOT for fields; behavior overview in [architecture.md](architecture.md) §9.

## config.json

| Item | Value |
|------|--------|
| Path | `%APPDATA%\GMTL Mouse Click\config.json` |
| Env fallback | If `APPDATA` unset → `LOCALAPPDATA`; if both unset → process cwd. Then join `GMTL Mouse Click\config.json` (single path, not multi-file search) |
| Corrupt file | Parse or read failure (other than file not found) → copied to `config.json.bak`, app starts with defaults |
| Atomic save | Written to `config.json.tmp`, then renamed over `config.json` |
| Schema | `AppConfig` in [`src/config.rs`](../../src/config.rs) |

### AppConfig fields (summary)

| Field | Notes |
|-------|--------|
| `left_click_color` / `right_click_color` | RGB; defaults (0,150,255) / (255,96,96) |
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
| `gif_fps` | 5, 10, 15 or 20 (other values snap to the nearest), default 10 |
| `snapshot_save_png` | default false — also save snapshots as PNG |
| `snapshot_output_path` | default `%USERPROFILE%\Downloads`; also used by `Ctrl+S` in annotate mode |
| `hotkeys` | `record` / `snapshot` / `annotate`, each `{ "modifiers", "key" }`; defaults `Ctrl+Alt+Shift+R` / `C` / `A` |

`hotkeys.*.modifiers` uses `RegisterHotKey` values (Alt 1, Ctrl 2, Shift 4, Win 8); `key` is a virtual-key code. The Shortcuts page cannot enter Win, so opening that page and saving drops a hand-edited Win modifier.

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
