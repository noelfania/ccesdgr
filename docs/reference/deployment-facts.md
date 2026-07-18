---
status: active
audience: developer
verified: 2026-07-18
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
| Package name | `ccesdgr` (see [Cargo.toml](../../Cargo.toml)) |

## Release profile

From [Cargo.toml](../../Cargo.toml):

```toml
[profile.release]
lto = true
codegen-units = 1
strip = true
```

## Runtime

- No webview or large runtime bundled
- Icon from `assets/icon-entry-3.png` at build time (`build.rs` → multi-size `.ico`, sizes 16/24/32/48/256)
- Exe + tray share resource ID 1 (blue background — readable on light and dark taskbars)
- Console subsystem disabled (`#![windows_subsystem = "windows"]`)

## Distribution (current)

Manual copy of `ccesdgr.exe`. GitHub Release / Store / WinGet — see [planning/backlog-ideas.md](../planning/backlog-ideas.md).
