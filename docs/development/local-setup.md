---
status: active
audience: developer
verified: 2026-07-04
---

# Local setup

## Requirements

- Windows 10/11
- Rust toolchain (edition 2021)

## Install Rust

```bash
winget install Rustlang.Rustup
rustc --version
cargo --version
```

## Build

From repository root:

```bash
cargo check
cargo build --release
```

Output: `target/release/ccesdgr.exe`

## Run

Launch `ccesdgr.exe` directly (portable, no installer). The app stays in the system tray.
