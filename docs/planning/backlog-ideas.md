---
status: active
audience: developer
verified: 2026-07-25
---

# Backlog ideas

Raw notes and open problems. **Shipped items** → [CHANGELOG.md](../../CHANGELOG.md).

## Defaults / UX polish

- Revisit effect radius / lifetime defaults (current: radius 72, lifetime 28)
- Settings live preview
- Click effect: hold trace until mouse up (toggle?)

## New features

- User-selectable tray/app icon (build already embeds `assets/icon-entry-3.png`)
- Clipboard snippets, mail templates
- Nickname watermark
- Recent captures popup (drag/copy from tray corner)
- Output naming rules beyond current `gif-YYYYMMDD-HHMMSS.gif` / `gif_output_path` / `ask_gif_save_path`

## Open problems

- GIF frame border thickness eats capture area at screen top
- Draw feels coarse; ellipse with Tab looks wrong; cross effect rounding?

## Performance / GIF policy

- Validate on other DPI/monitor setups
- Default scale/FPS policy (memory vs quality)
- 1080p 15fps, 60s GIF under ~1GB RAM target
- Drop policy: duplicate last frame instead of dropping when encoder lags
- User-configurable FPS (default today is ~10 FPS / 100ms)
- Min Windows 10 build TBD
- Encoder thread safety via mutex

## Refactor ideas

- `capture_ui` → `gif_record`
- `select_rect` + `snip` → `snapshot`

## Distribution (future)

- GitHub Pages for intro, GitHub Release for exe
- Microsoft Store / WinGet
- Code signing / SmartScreen notes

## Done (see CHANGELOG / incidents)

- English UI, global hotkeys (`Ctrl+Alt+Shift+C` / `A`), pen cursor in draw mode
- Striped GIF frame, virtual-screen clamp, `capture_frame` persistence
- Draw render performance — committed buffer ([incident-draw-render-performance.md](../reference/incident-draw-render-performance.md))
- Default GIF save folder + optional Save As (`gif_output_path`, `ask_gif_save_path`)
