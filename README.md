<p align="center">
  <img src="assets/app-icon.png" width="96" alt="ccesdgr — line icon: frame, cursor and click dot">
</p>

# ccesdgr

GMTL Mouse Click — Win32 tray app for click visualization, GIF recording, snapshots, and screen annotation.

App icon: monochrome line icon traced from the `assets/icon001-1.svg` sketch — black (`assets/app-icon*.png`, exe + light taskbar) and white (`assets/app-icon-dark*.png`, dark taskbar); 16–48 px are drawn on the pixel grid for a crisp tray. The tray follows the taskbar theme and switches live. Regenerate with `python tools/icon/gen_icon.py` (Pillow). Build embeds two 10-size `.ico` resources (ID 1 black, ID 2 white).

**Documentation:** [docs/README.md](docs/README.md)  
**Architecture:** [docs/reference/architecture.md](docs/reference/architecture.md)  
**Changelog:** [CHANGELOG.md](CHANGELOG.md)

## Quick start

```bash
cargo check
cargo build --release
```

Output: `target/release/ccesdgr.exe`

Tray-resident; settings / Record GIF / Snapshot / Annotate via tray menu. Default hotkeys (change in Settings > Shortcuts): `Ctrl+Alt+Shift+R` (record GIF: open → start → stop), `Ctrl+Alt+Shift+C` (snapshot), `Ctrl+Alt+Shift+A` (annotate; `F1` in annotate mode lists tools).

See [docs/development/local-setup.md](docs/development/local-setup.md) for Rust install details.
