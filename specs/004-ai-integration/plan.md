# Implementation Plan: AI Integration Layer

**Branch**: `004-ai-integration` | **Date**: 2026-03-19 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/004-ai-integration/spec.md`

## Summary

Add an AI assistant to ArcTerm with two interaction modes: an AI pane for
conversational help (reads sibling pane context — scrollback, CWD, exit code)
and a command overlay for one-shot command generation. Default backend is
Ollama (local, zero-config), with Claude API as an opt-in alternative.
A new `arcterm-ai` crate handles LLM communication and context extraction,
while GUI components (AI pane via TermWizTermTab, command overlay) live
in the GUI crate.

## Technical Context

**Language/Version**: Rust (edition 2021)
**Primary Dependencies**: http_req (already present), serde_json (already present), smol (already present)
**Storage**: N/A — conversation history is in-memory, per-session only
**Testing**: `cargo test --all` + manual testing with Ollama
**Target Platform**: macOS, Linux, Windows
**Project Type**: Desktop application — new AI subsystem
**Performance Goals**: First token < 2s, 60fps during streaming, overlay cycle < 5s
**Constraints**: Must not require tokio (use smol). Must not send data to remote APIs without explicit config. Must not block GUI thread.
**Scale/Scope**: 1 new crate (~2000 lines), GUI components in wezterm-gui, 3 new KeyAssignment variants

## Constitution Check

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Upstream Compatibility | PASS | New `arcterm-ai` crate. GUI components are new files in wezterm-gui (overlay + TermWizTermTab). Minimal changes to existing files (KeyAssignment enum + config). |
| II. Security by Default | PASS | FR-014: no remote API calls without explicit config. Ollama is local-only by default. API keys stored in config file, not exposed to plugins. |
| III. Local-First AI | PASS | Core design principle. Ollama default, Claude opt-in. Works fully offline. Degrades gracefully when no LLM available. Zero config needed. |
| IV. Extension Isolation | PASS | AI subsystem is isolated in its own crate. Does not interfere with Lua plugins or WASM plugins. |
| V. Test Preservation | PASS | All existing tests must pass. SC-007 explicitly requires `cargo test --all` green. |
| VI. Minimal Surface Area | PASS | Two interaction modes (pane + overlay), each optimized for its use case. Config has sensible defaults. No unnecessary abstractions. |

**Gate result: PASS** — no violations.

## Project Structure

### Documentation (this feature)

```text
specs/004-ai-integration/
├── plan.md              # This file
├── research.md          # Pane context, HTTP client, overlay patterns research
├── data-model.md        # AI Pane, Command Overlay, PaneContext, LlmBackend entities
├── quickstart.md        # Test scenarios with Ollama
└── tasks.md             # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

```text
arcterm-ai/                              # New crate
├── Cargo.toml
├── src/
│   ├── lib.rs                           # Crate root, public API
│   ├── backend/
│   │   ├── mod.rs                       # LlmBackend trait definition
│   │   ├── ollama.rs                    # Ollama REST API client (streaming)
│   │   └── claude.rs                    # Claude Messages API client (streaming)
│   ├── context.rs                       # PaneContext extraction from mux panes
│   ├── config.rs                        # AiConfig types + defaults
│   ├── prompts.rs                       # System prompt templates
│   └── destructive.rs                   # Destructive command pattern matching
└── tests/
    └── backend_tests.rs                 # LLM backend unit tests (mocked)

# GUI components (in existing crate)
wezterm-gui/src/
├── overlay/
│   └── ai_command_overlay.rs            # Command overlay UI (new file)
└── ai_pane.rs                           # AI pane TermWizTermTab (new file)

# Modified existing files (minimal)
config/src/keyassignment.rs              # Add OpenAiPane, ToggleCommandOverlay, RefreshAiContext
wezterm-gui/src/main.rs                  # Wire AI pane + overlay
Cargo.toml                               # Add arcterm-ai to workspace
```

**Structure Decision**: New `arcterm-ai` crate for backend logic, context
extraction, and config. GUI components are new files in `wezterm-gui`.
Only 2 existing files need modification (keyassignment enum + main.rs wiring).

## Complexity Tracking

No constitution violations — this section is empty.
