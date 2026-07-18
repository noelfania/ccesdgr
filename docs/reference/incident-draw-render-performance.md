---
status: archived
audience: developer
verified: 2026-07-04
---

# Incident: draw render replay performance

## Symptom

Annotation mode slows as more strokes are added; drawing feels to pause then catch up.

## Cause

`render()` clones full background and all commands every frame; replays entire history. Blur strokes are especially expensive.

## Status

**Open** — documented improvement direction (committed buffer + preview layer). Not fully implemented.

## Recommended direction

- `committed_buffer` for finalized strokes
- Preview only `draft` + cursor on mouse move
- Undo via snapshot or limited recompose

## Related

- `src/draw.rs`

Original note: `doc/issues/draw-render-replay-performance.md`
