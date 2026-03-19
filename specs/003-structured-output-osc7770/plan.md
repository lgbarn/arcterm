# Implementation Plan: Structured Output via OSC 7770

**Branch**: `003-structured-output-osc7770` | **Date**: 2026-03-19 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/003-structured-output-osc7770/spec.md`

## Summary

Add OSC 7770 escape sequence support to ArcTerm for rendering structured content
(syntax-highlighted code, collapsible JSON trees, colored diffs, inline images)
directly in the terminal. The key insight from research is that code/JSON/diff
blocks are rendered as ANSI SGR-colored text through the existing terminal pipeline
(no custom GPU rendering), while images reuse the existing iTerm2 image
infrastructure. A new `arcterm-structured-output` crate handles payload parsing
and content-to-SGR conversion.

## Technical Context

**Language/Version**: Rust (edition 2021)
**Primary Dependencies**: syntect v5.3 (syntax highlighting), serde_json (payload parsing), base64 (image decoding)
**Storage**: N/A — structured blocks stored as normal terminal lines in scrollback
**Testing**: `cargo test --all` + test fixtures (shell scripts emitting OSC 7770)
**Target Platform**: macOS, Linux, Windows
**Project Type**: Desktop application — new terminal feature
**Performance Goals**: Code block render < 50ms for 500 lines, JSON tree interactive at 1000+ keys, 60fps with 10+ blocks visible
**Constraints**: Must not break existing escape sequence handling. Must degrade silently in non-ArcTerm terminals.
**Scale/Scope**: 1 new crate (~1500 lines), 2 modified files in existing crates, protocol specification

## Constitution Check

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Upstream Compatibility | PASS | New `arcterm-structured-output` crate. Only 2 upstream files modified (OSC enum + handler dispatch) with minimal, surgical changes. |
| II. Security by Default | PASS | Payload size limits enforced. Image data validated before rendering. No network access or filesystem operations. |
| III. Local-First AI | N/A | No AI features in this change. |
| IV. Extension Isolation | PASS | New OSC handler is isolated — unknown types fall through to plain text, malformed payloads are silently discarded. No impact on existing OSC handlers. |
| V. Test Preservation | PASS | All existing tests must pass. SC-006 explicitly requires `cargo test --all` green. |
| VI. Minimal Surface Area | PASS | Reuses existing text rendering and image pipelines. No custom GPU code. Four content types — each solves a specific, stated developer need. |

**Gate result: PASS** — no violations.

## Project Structure

### Documentation (this feature)

```text
specs/003-structured-output-osc7770/
├── plan.md                          # This file
├── research.md                      # OSC parser, renderer, syntect research
├── data-model.md                    # StructuredBlock, BlockType, payload schema
├── quickstart.md                    # Shell test commands
├── contracts/
│   └── osc-7770-protocol.md         # Full protocol specification
└── tasks.md                         # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

```text
arcterm-structured-output/                   # New crate
├── Cargo.toml
├── src/
│   ├── lib.rs                               # Crate root, public render() API
│   ├── payload.rs                           # JSON payload parsing + validation
│   ├── code.rs                              # Syntax highlighting via syntect
│   ├── json_tree.rs                         # JSON tree rendering with collapse markers
│   ├── diff.rs                              # Unified diff parsing + side-by-side coloring
│   └── image.rs                             # Base64 decode + image-to-cells conversion
└── tests/
    └── render_tests.rs                      # Unit tests for each content type

# Modified existing files (minimal — 2 files)
wezterm-escape-parser/src/osc.rs             # Add OSC 7770 variant to enum + macro
term/src/terminalstate/performer.rs          # Add handler in osc_dispatch()

# Workspace
Cargo.toml                                   # Add arcterm-structured-output to members
```

**Structure Decision**: New `arcterm-structured-output` crate at workspace root.
Only 2 existing files modified: the OSC enum definition and the dispatch handler.
The crate's public API is a single `render(payload: &str) -> Vec<Action>` function
that returns terminal actions the existing state machine can process natively.

## Complexity Tracking

No constitution violations — this section is empty.
