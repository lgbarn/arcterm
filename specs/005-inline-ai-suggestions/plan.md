# Implementation Plan: Inline AI Command Suggestions

**Branch**: `005-inline-ai-suggestions` | **Date**: 2026-03-19 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/005-inline-ai-suggestions/spec.md`

## Summary

Add Warp-style inline AI command suggestions to ArcTerm. As the user types
at a shell prompt, a debounced query is sent to the local LLM (Ollama).
The response renders as dimmed ghost text after the cursor. Tab accepts,
Escape dismisses, continued typing refines. Uses OSC 133 for reliable prompt
detection, CopyOverlay pattern for ghost text rendering, and the existing
smol Timer + cookie debounce pattern from the search overlay.

## Technical Context

**Language/Version**: Rust (edition 2021)
**Primary Dependencies**: arcterm-ai (existing), smol (existing timer/unblock), termwiz (rendering)
**Storage**: N/A — suggestions are ephemeral (not persisted)
**Testing**: `cargo test --all` + manual testing with Ollama
**Target Platform**: macOS, Linux, Windows
**Project Type**: Desktop application — new interactive feature
**Performance Goals**: Ghost text within 1s of pause, Tab-accept <50ms, 60fps during queries, zero typing latency impact
**Constraints**: Must not block GUI thread. Tab must coexist with shell completion. Must be invisible when LLM unavailable.
**Scale/Scope**: ~500 lines new code across arcterm-ai + wezterm-gui

## Constitution Check

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Upstream Compatibility | PASS | New overlay file in wezterm-gui + new module in arcterm-ai. No upstream file modifications beyond KeyAssignment (already added). |
| II. Security by Default | PASS | FR-013: no remote API without explicit config. Context sent to local Ollama only by default. |
| III. Local-First AI | PASS | Core design — Ollama default, zero config, invisible when unavailable. |
| IV. Extension Isolation | PASS | Suggestion overlay is a pane wrapper that doesn't interfere with other overlays or plugins. |
| V. Test Preservation | PASS | SC-008: cargo test --all must remain green. |
| VI. Minimal Surface Area | PASS | One overlay, one module, reuses existing LLM backend and context extraction. Tab/Escape only intercepted when suggestion visible. |

**Gate result: PASS**

## Project Structure

### Source Code

```text
arcterm-ai/src/
└── suggestions.rs              # NEW: debounce logic, query formatting, response cleaning

wezterm-gui/src/
└── suggestion_overlay.rs       # NEW: ghost text pane overlay, key table, rendering
```

### Key Integration Points

```text
term/src/terminalstate/         # Read SemanticType zones for prompt detection
wezterm-gui/src/termwindow/     # Key dispatch + overlay assignment
arcterm-ai/src/backend/         # Reuse LlmBackend for suggestion queries
arcterm-ai/src/context.rs       # Reuse PaneContext for terminal context
```

**Structure Decision**: Two new files only. All LLM logic in `arcterm-ai`,
all rendering in `wezterm-gui`. Reuses existing backend, context, and
overlay infrastructure. Zero new crates.
