<p align="center">
  <img src="assets/app-icon.png" width="96" alt="ccesdgr — mouse with cursor and click dot icon">
</p>

# ccesdgr

GMTL Mouse Click — Win32 tray app for click visualization, GIF recording, snapshots, and screen annotation.

App icon: `assets/app-icon.png` (blue two-tone mouse, white cursor, red click dot; transparent background) plus hand-tuned tray sizes `assets/app-icon-{16,20,24}.png`. Regenerate with `python tools/icon/gen_icon.py` (Pillow). Build embeds a 10-size `.ico` for the `.exe` and tray.

**Documentation:** [docs/README.md](docs/README.md)  
**Architecture:** [docs/reference/architecture.md](docs/reference/architecture.md)  
**Changelog:** [CHANGELOG.md](CHANGELOG.md)

## Quick start

```bash
cargo check
cargo build --release
```

Output: `target/release/ccesdgr.exe`

Tray-resident; settings / Record GIF / Snapshot / Annotate via tray menu. Hotkeys: `Ctrl+Alt+Shift+R` (record GIF: open → start → stop), `Ctrl+Alt+Shift+C` (snapshot), `Ctrl+Alt+Shift+A` (annotate; `F1` in annotate mode lists tools).

See [docs/development/local-setup.md](docs/development/local-setup.md) for Rust install details.
