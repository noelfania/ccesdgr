---
status: active
audience: developer
verified: 2026-07-25
---

# Verify before push

Minimum checks before pushing code:

```bash
cargo check
cargo build --release
```

## Manual smoke (when UI changed)

- Tray menu opens and actions work (English labels)
- Click effect visible (if enabled)
- Settings save to `config.json`
- GIF frame, snapshot, and annotate flows (if touched)
- Hotkeys if touched: `Ctrl+Alt+Shift+C` (snapshot), `Ctrl+Alt+Shift+A` (annotate)

## Automated tests

```bash
cargo test                 # unit tests (pure logic)
cargo test -- --ignored    # + 1s real screen recording smoke test (needs a desktop session)
```
