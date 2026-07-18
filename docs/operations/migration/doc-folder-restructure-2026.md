---
status: archived
audience: developer
verified: 2026-07-04
---

# doc/ → docs/ restructure (2026-07)

## What changed

- Removed `doc/` at repo root.
- Added `docs/` with `development/`, `operations/migration/`, `planning/`, `reference/`.
- Added root [CHANGELOG.md](../../../CHANGELOG.md) (Keep a Changelog).
- Updated [`.cursor/rules/project-standards.mdc`](../../../.cursor/rules/project-standards.mdc) architecture link.

## Mapping

| Old | New |
|-----|-----|
| `doc/app-detailed-design.md` | `docs/reference/architecture.md` |
| `doc/upcoming-release-note.md` | `docs/planning/backlog-ideas.md` |
| `doc/issues/*.md` | `docs/reference/incident-*.md` |
| `doc/images/gmtl.svg` | `docs/reference/images/gmtl.svg` |

No `operations/routine/` — no recurring deploy/runbook for this project.
