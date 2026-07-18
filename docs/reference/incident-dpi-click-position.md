---
status: archived
audience: developer
verified: 2026-07-04
---

# Incident: DPI click position mismatch

## Problem

Click effect center offset from actual click, worse with non-100% DPI or mixed monitor scaling.

## Cause

Mouse hook coordinates and overlay window used different DPI awareness.

## Fix

`SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)` at startup, fallback `SetProcessDPIAware`. See `src/main.rs`.

## Related

- `src/main.rs`
- `src/effect.rs`

Original note: `doc/issues/dpi-click-position-fix.md`
