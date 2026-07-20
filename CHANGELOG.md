# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) where applicable.

## [Unreleased]

### Added

- `docs/` folder structure (`development/`, `operations/migration/`, `planning/`, `reference/`)
- Root `CHANGELOG.md` and per-doc YAML frontmatter
- Folder READMEs under `docs/`
- Reference docs: `config-storage.md`, `deployment-facts.md`
- Pen cursor in screen annotation (draw) mode
- Striped GIF capture frame (idle: white/black, recording: red/black)
- Global hotkeys: `Ctrl+Alt+Shift+C` (snapshot), `Ctrl+Alt+Shift+A` (annotate)
- `capture_frame` in `config.json` — remembers GIF frame size/position with monitor fallback

### Changed

- Migrated `doc/` → `docs/`; updated Cursor rule links
- English UI strings (tray, settings, capture UI, error messages); code comments remain Korean
- GIF capture frame clamps to virtual screen when dragged off-screen (no black capture padding)
- Cursor architecture link points to `docs/reference/architecture.md`
- App/tray icon source switched to `assets/icon-entry-3.png` (single multi-size ICO; no theme-split tray icons)
- Embed ComCtl32 v6 application manifest (`assets/app.manifest`) and call `InitCommonControlsEx` for themed Win32 controls
- Settings: `ask_gif_save_path` — when on, prompt for GIF filename/folder after recording ends (Cancel discards)

## [0.1.12] - (prior releases)

Earlier history not recorded in this file. See git log.
