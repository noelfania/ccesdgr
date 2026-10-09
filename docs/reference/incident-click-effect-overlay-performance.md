---
status: archived
audience: developer
verified: 2026-07-04
---

# Incident: click effect fullscreen overlay performance

## Symptom

Do Not Disturb flickered on each click; system felt sluggish during rapid clicks.

## Cause

Effect window resized to full virtual screen each click; 16ms full-screen RGBA buffer + `UpdateLayeredWindow`.

## Fix

Minimal bounding rect around active effects only; clamp to virtual screen. See `src/effect.rs`.

## Related

- `src/effect.rs`

Original note: `doc/issues/click-effect-fullscreen-overlay-performance.md` (path retired — see [operations/migration/doc-folder-restructure-2026.md](../operations/migration/doc-folder-restructure-2026.md))
