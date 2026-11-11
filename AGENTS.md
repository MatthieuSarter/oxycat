# AGENTS.md

- Design spec: `docs/superpowers/specs/2026-11-11-oxycat-design.md`; implementation plan:
  `docs/superpowers/plans/2026-11-11-oxycat.md`. Read both before changing behavior.
- `src/core/` holds pure logic only: no filesystem, database, clock, environment or locale access. Callers pass
  those values in.
- Never keep a SQLite connection open between operations (open, query, drop): an open handle blocks the atomic
  rename that replaces a catalog on Windows.
- Every user-visible string goes through `src/i18n.rs` (FR/EN/ES fields of `Strings`).
- Quality gate: `just lint`, `just test`, `just coverage`.
