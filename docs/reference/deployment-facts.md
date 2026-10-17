---
status: active
audience: developer
verified: 2026-07-25
---

# Deployment facts

Facts only — no deploy runbook (none for this project).

## Target

- Windows 10/11, Win32 native
- Single process, tray-resident
- Portable exe (no installer)

## Build output

| Item | Value |
|------|--------|
| Command | `cargo build --release` |
| Binary | `target/release/ccesdgr.exe` |
| Package name / version | `ccesdgr` — see [Cargo.toml](../../Cargo.toml) |
| Manifest assembly version | `assets/app.manifest` (keep in sync with package version) |

## Release profile

From [Cargo.toml](../../Cargo.toml):

```toml
[profile.release]
lto = true
codegen-units = 1
strip = true
```

## Build-time assets ([`build.rs`](../../build.rs))

| Source | Output |
|--------|--------|
| `assets/app-icon.png` + `assets/app-icon-{16,20,24}.png` | Multi-size `.ico` (16/20/24 hand-tuned; 32/40/48/64/96/128/256 from 1024 master), resource ID 1. Source: `tools/icon/gen_icon.py` |
| `assets/draw-pen.png` | CUR + RGBA bin for tinted pen cursor |
| `assets/app.manifest` | Embedded (DPI PerMonitorV2, ComCtl32 v6) |

Build deps: `winresource`, `image` (png), `ico`.

## Runtime

- No webview or large runtime bundled
- Exe + tray share resource ID 1 (blue background — readable on light and dark taskbars)
- Console subsystem disabled (`#![windows_subsystem = "windows"]`)

## Distribution (current)

Manual copy of `ccesdgr.exe`. GitHub Release / Store / WinGet — see [planning/backlog-ideas.md](../planning/backlog-ideas.md).
