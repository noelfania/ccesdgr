---
status: active
audience: developer
---

# Backlog ideas

Raw notes and open problems. **Shipped items** → [CHANGELOG.md](../../CHANGELOG.md).

## Simple is best (defaults)

- Effect radius default 30 → 20
- Effect speed default 52 → 55

## New features

- Custom tray/app icon
- Clipboard snippets, mail templates
- Nickname watermark
- Recent captures popup (drag/copy from tray corner)
- Settings live preview
- Click effect: hold trace until mouse up (toggle?)
- Output naming rules and default save location

## Open problems

- GIF frame border thickness eats capture area at screen top
- Draw feels coarse; ellipse with Tab looks wrong; cross effect rounding?
- Draw slows as stroke count grows (see [incident-draw-render-performance.md](../reference/incident-draw-render-performance.md))

## Performance / GIF policy

- Validate on other DPI/monitor setups
- Default scale/FPS policy (memory vs quality)
- 1080p 15fps, 60s GIF under ~1GB RAM target
- Drop policy: duplicate last frame instead of dropping when encoder lags
- User-configurable FPS (default 15)
- Min Windows 10 build TBD
- Encoder thread safety via mutex

## Refactor ideas

- `capture_ui` → `gif_record`
- `select_rect` + `snip` → `snapshot`

## Distribution (future)

- GitHub Pages for intro, GitHub Release for exe
- Microsoft Store / WinGet
- Code signing / SmartScreen notes

## Done (see CHANGELOG)

- English UI, global hotkeys (Ctrl+Alt+Shift+C/A), pen cursor in draw mode
- Striped GIF frame, virtual-screen clamp, capture frame persistence
