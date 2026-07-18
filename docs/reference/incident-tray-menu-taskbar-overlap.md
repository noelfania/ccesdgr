---
status: archived
audience: developer
verified: 2026-07-04
---

# Incident: tray menu taskbar overlap

## Symptom

Tray context menu overlapped taskbar on Windows 11; taskbar appeared wrong height until click.

## Cause

`TrackPopupMenu` with `TPM_BOTTOMALIGN` anchored menu bottom to cursor inside taskbar.

## Fix

When cursor is inside taskbar rect, use `menu_y = taskbar.top - 1`. See `src/tray.rs`.

## Related

- `src/tray.rs`

Original note: `doc/issues/tray-menu-taskbar-overlap.md`
