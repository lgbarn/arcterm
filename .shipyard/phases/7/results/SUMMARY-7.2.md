---
plan: "7.2"
phase: ai-integration
status: complete
date: 2026-03-15
---

# SUMMARY-7.2 — MCP Tool Discovery + Plan Status Layer + AI Keybindings

## What Was Built

### Task 1 — MCP Tool Discovery via OSC 7770

**Files changed:** `arcterm-vt/src/handler.rs`, `arcterm-vt/src/processor.rs`, `arcterm-app/src/terminal.rs`, `arcterm-app/src/main.rs`, `arcterm-app/Cargo.toml`, `arcterm-plugin/src/manager.rs`

Two new OSC 7770 subtypes were added to the VT layer:

- `tools/list` — sets a sentinel in `GridState.tool_queries` drain buffer. The app layer drains these, calls `PluginManager::list_tools()`, serializes tool schemas as JSON, base64-encodes the result, and writes `ESC ] 7770 ; tools/response ; <base64_json> BEL` back to the PTY.
- `tools/call` — parses `name=` and `args=` (base64-decoded) from the OSC params, pushes to `GridState.tool_calls`. The app layer drains these, calls `PluginManager::call_tool(name, args)`, and writes `ESC ] 7770 ; tools/result ; result=<base64_json> BEL` back.

`PluginManager.call_tool()` is a stub returning `{"error":"tool invocation not yet implemented"}` — full WASM invocation is deferred to Phase 8. The OSC round-trip is complete.

Seven unit tests in `arcterm-vt` verify: tools/list sets the flag, accumulation, drain clearing, tools/call parses name and decoded args, malformed calls are silently ignored.

**Deviation:** `ToolSchema` (WIT-generated) does not implement `serde::Serialize`. Serialization is done manually by constructing a JSON string from the struct fields, avoiding a new derive dependency on the generated type.

### Task 2 — Leader+a and Leader+p Keybindings

**Files changed:** `arcterm-app/src/keymap.rs`, `arcterm-app/src/main.rs`

Two new `KeyAction` variants were added:

- `KeyAction::JumpToAiPane` — dispatched by Leader then `a` (no ctrl). Wired to focus `last_ai_pane` if the pane still exists in the pane map.
- `KeyAction::TogglePlanView` — dispatched by Leader then `p`. Wired to toggle `plan_view` between `None` and `Some(PlanViewState::new(summaries))`.

The double-tap leader check (Ctrl+a again while pending) runs first in the LeaderPending branch, so `"a"` without ctrl is safely disambiguated. Both actions reset `is_leader_pending()` to false.

Two new unit tests verify each binding and state machine reset. All 32 keymap tests pass.

### Task 3 — Plan Status Layer

**Files created:** `arcterm-app/src/plan.rs`
**Files changed:** `arcterm-app/src/main.rs`

`plan.rs` contains:

- `PlanSummary { phase, completed, total, file_path }` — one per plan file.
- `parse_plan_summary_from_str(text, path)` — scans for `[x]`/`[X]`/`[ ]` patterns. Extracts `phase:` from YAML frontmatter if present. Returns `None` for files with no checkboxes.
- `discover_plan_files(workspace_root)` — scans `.shipyard/PLAN-*.md` (sorted), recursively through `.shipyard/phases/`, then `PLAN.md`, then `TODO.md`.
- `PlanStripState { summaries, last_updated }` — ambient status bar data with `discover()`, `refresh()`, and `strip_text()` methods.
- `PlanViewState { entries, selected }` — expanded modal overlay following the PaletteState pattern, with `render_quads()` and `render_text()` helpers.

**AppState additions:** `plan_strip`, `plan_view`, `plan_watcher`, `plan_watcher_rx`, `workspace_root`.

**Geometry:** `pane_area()` subtracts one cell row from the available height when `plan_strip` is `Some`, so panes are pushed up.

**Rendering:**
- Plan strip: a single `OverlayQuad` spanning the window bottom + a text label `"Phase {phase} | {completed}/{total}"`.
- Plan view: full-screen dim + centered box + title bar + per-entry rows + selection highlight, appended to the same `overlay_quads` / `palette_text` vectors used by the command palette.

**File watcher:** uses `notify::recommended_watcher` with `std::sync::mpsc::channel` (identical pattern to `config.rs`). Watches `.shipyard/` (recursive), `PLAN.md`, `TODO.md`. Polled in `about_to_wait` via `try_recv`.

Twelve unit tests verify checkbox counting, phase extraction, strip text formatting, and edge cases (empty file, no checkboxes, uppercase X, multiple checkboxes per line).

## Verification Results

```
# Task 1
cargo test --package arcterm-vt -- tools
  7 passed, 0 failed

# Task 2
cargo test --package arcterm-app -- keymap::tests
  32 passed, 0 failed (includes 2 new tests)

# Task 3
cargo test --package arcterm-app -- plan::
  12 passed, 0 failed

cargo build --package arcterm-app
  Finished dev profile — 5 warnings (all dead_code on PlanViewState helpers)
```

## Commits

1. `shipyard(phase-7): implement MCP tool discovery via OSC 7770 tools/list and tools/call` — b6fb69c
2. `shipyard(phase-7): add Leader+a (JumpToAiPane) and Leader+p (TogglePlanView) keybindings` — cd6d737
3. `shipyard(phase-7): add plan status layer with PlanStripState, PlanViewState, and file watcher` — 136b840

## Deferred / Known Gaps

- `PluginManager::call_tool()` returns a stub. Full WASM invocation deferred to Phase 8.
- `PlanViewState::select_up/down` are implemented but not yet wired to keyboard input in the modal handler. Adding keyboard navigation to the plan view is a Phase 8 UX enhancement.
- `ToolSchema` from WIT bindgen lacks `serde::Serialize`. Serialization is manual; a proper wrapper type could be added if the schema structure grows.
