# Implementation Plan: Warp-Style AI UX

**Branch**: `006-warp-style-ai-ux` | **Date**: 2026-03-19 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/006-warp-style-ai-ux/spec.md`

## Summary

Three Warp-inspired UX improvements: (1) compact bottom command panel using
the tab-bar rendering pattern, (2) agent mode with `#` prefix interception
via the AI overlay shim, and (3) step execution monitoring via OSC 133;D
CommandComplete alert. Modifies existing rendering pipeline, terminal state
machine, and overlay infrastructure.

## Technical Context

**Language/Version**: Rust (edition 2021)
**Primary Dependencies**: arcterm-ai (existing), termwiz, mux
**Storage**: N/A — ephemeral UI state
**Testing**: `cargo test --all` + manual testing per quickstart.md
**Target Platform**: macOS, Linux, Windows
**Project Type**: Desktop application — GUI rendering + terminal state
**Performance Goals**: Panel renders at 60fps, step completion detected <100ms
**Constraints**: Must not break existing tab bar, must gate `#` on OSC 133
**Scale/Scope**: ~1500 lines across render pipeline, overlay, and term crate

## Constitution Check

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Upstream Compatibility | PASS | Panel is new render code. OSC 133;D handler fills an existing stub. Alert enum addition is minimal. |
| II. Security by Default | PASS | Agent mode requires explicit `#` prefix. No remote API without config. |
| III. Local-First AI | PASS | Uses existing Ollama backend. Zero config needed. |
| IV. Extension Isolation | PASS | Panel and agent mode don't interfere with Lua/WASM plugins. |
| V. Test Preservation | PASS | SC-006 requires cargo test --all green. |
| VI. Minimal Surface Area | PASS | Panel follows tab-bar pattern. Agent mode reuses overlay shim. |

**Gate result: PASS**

## Project Structure

### Modified Files

```text
# Rendering (compact bottom panel)
wezterm-gui/src/termwindow/render/ai_panel.rs    # NEW: paint_ai_panel
wezterm-gui/src/termwindow/render/paint.rs        # Add paint_ai_panel call
wezterm-gui/src/termwindow/resize.rs              # Subtract ai_panel_height
wezterm-gui/src/termwindow/mod.rs                 # Add ai_panel state + helper

# Agent mode (# prefix interception)
arcterm-ai/src/agent.rs                           # NEW: step planning, execution state
wezterm-gui/src/suggestion_overlay.rs             # Extend: Enter interception for #

# Step execution monitoring
term/src/terminal.rs                              # Add Alert::CommandComplete
term/src/terminalstate/mod.rs                     # Add last_command_exit_status
term/src/terminalstate/performer.rs               # Fill 133;D handler (line 913)

# Existing overlay
wezterm-gui/src/overlay/ai_command_overlay.rs     # Refactor: use panel instead of overlay
```

**Key insight**: The compact panel does NOT create a new pane or overlay.
It's a chrome region (like the tab bar) painted by the render pipeline.
The terminal viewport shrinks to accommodate it.
