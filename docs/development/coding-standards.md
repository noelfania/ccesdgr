---
status: active
audience: developer
verified: 2026-07-25
---

# Coding standards

Summary of project conventions. Full rules for Cursor: [`.cursor/rules/project-standards.mdc`](../../.cursor/rules/project-standards.mdc).

## Style

- Clear, conventional identifiers; avoid unnecessary abstraction.
- **Comments in Korean** when needed; keep them short and factual.
- **User-facing UI strings in English.**

## Quality

- Release profile uses `lto`, `strip`, `codegen-units = 1` — keep binaries lean.
- Click effect, draw, and GIF paths are pixel-heavy — avoid extra full-screen repaints or copies.
- After changes, run `cargo build --release` at minimum.

## Architecture reference

Module map and sequences: [../reference/architecture.md](../reference/architecture.md)
