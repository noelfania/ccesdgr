---
status: active
audience: developer
verified: 2026-07-04
---

# Verify before push

Minimum checks before pushing code:

```bash
cargo check
cargo build --release
```

## Manual smoke (when UI changed)

- Tray menu opens and actions work
- Click effect visible (if enabled)
- Settings save to `config.json`
- GIF frame, snapshot, and annotate flows (if touched)

No automated test suite yet.
