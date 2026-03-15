# Phase 1 Context — Design Decisions

## VT Parser
**Decision:** Use the `vte` crate as a dependency, extend with our own layer for OSC 7770 in Phase 4.
**Rationale:** Faster path to working terminal, battle-tested parsing. We add structured protocol support on top later.

## PTY Handling
**Decision:** Use the `portable-pty` crate.
**Rationale:** Handles Unix/Windows PTY differences. Used by WezTerm, proven in production.

## Workspace Structure
**Decision:** Multi-crate Cargo workspace.
**Crates:**
- `arcterm-vt` — VT parser and terminal state machine
- `arcterm-pty` — PTY allocation and shell spawning
- `arcterm-render` — wgpu renderer, glyph atlas, text shaping
- `arcterm-core` — Terminal grid model, event system, shared types
- `arcterm-app` — Application entry point, window management, input handling
**Rationale:** Clean boundaries, parallel compilation, easier testing per subsystem.

## CI Pipeline
**Decision:** Include GitHub Actions CI in Phase 1.
**Scope:** Build + test + clippy on macOS, Linux, Windows.
**Rationale:** Catch cross-platform issues early since wgpu targets all three.
