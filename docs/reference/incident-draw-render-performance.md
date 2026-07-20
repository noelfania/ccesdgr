---
status: archived
audience: developer
verified: 2026-07-20
---

# Incident: draw render replay performance

## Symptom

Annotation mode slows as more strokes are added (especially after blur); drawing feels to pause then catch up.

## Cause

`render()` cloned full background and replayed **all** commands every mouse-move frame. Blur rects re-ran expensive neighborhood averaging every frame.

## Fix (2026-07-20)

- `DrawState.committed`: finalized strokes baked once on commit
- `render()`: clone `committed` + composite **draft only**
- Undo: `recompose_committed()` from `background` + `commands`
- Mouse-move without draft skips `render` (cursor via `SetCursor` only)

## Related

- `src/draw.rs`
