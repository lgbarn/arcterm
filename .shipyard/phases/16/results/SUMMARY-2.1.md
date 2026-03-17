# SUMMARY-2.1.md — Phase 16, Plan 2.1: Command Overlay

**Date:** 2026-03-17
**Branch:** main
**Status:** Complete

---

## What Was Done

### Task 1: CommandOverlay State Machine

**File created:** `arcterm-app/src/command_overlay.rs`

Implemented the full state machine as specified:

- `OverlayAction` enum: `UpdateQuery`, `Submit`, `Accept(String)`, `Close`, `Noop`
- `OverlayPhase` enum: `Input`, `Loading`, `Result(String)`, `Error(String)`
- `CommandOverlayState` struct with `handle_key()`, `set_result()`, `set_error()`
- 12 inline tests — all pass

Registered `mod command_overlay;` in `arcterm-app/src/main.rs` after `mod ollama;`.

**Commit:** `b0e8ee4` — `shipyard(phase-16): add CommandOverlay state machine with input/loading/result phases`

---

### Task 2: Wire Command Overlay into Keymap and AppState

**Files modified:** `arcterm-app/src/keymap.rs`, `arcterm-app/src/main.rs`

Changes made:

1. Added `OpenCommandOverlay` variant to the `KeyAction` enum in `keymap.rs` (after `CrossPaneSearch`).

2. Changed `Ctrl+Space` mapping in the Normal state from `OpenPalette` to `OpenCommandOverlay`. The existing keymap test `ctrl_space_opens_palette` was updated to `ctrl_space_opens_command_overlay` to reflect the new binding.

3. Added two new fields to `AppState`:
   - `command_overlay: Option<command_overlay::CommandOverlayState>` — overlay state, initialized to `None`
   - `ollama_result_rx: Option<mpsc::Receiver<Result<String, String>>>` — channel for async Ollama results

4. Added `KeyAction::OpenCommandOverlay` dispatch in `dispatch_action`:
   - Opens the overlay if it is not already open
   - Returns `DispatchOutcome::Redraw`

5. Added modal key routing block (before search_overlay routing) in the keyboard event handler:
   - `OverlayAction::Close` → overlay cleared
   - `OverlayAction::UpdateQuery` → overlay put back, redraw
   - `OverlayAction::Submit` → spawns a tokio task calling `OllamaClient::generate()`, sends result back via `mpsc::channel(1)`; the channel receiver is stored in `ollama_result_rx`
   - `OverlayAction::Accept(cmd)` → writes `cmd + "\n"` to the focused pane's PTY input
   - `OverlayAction::Noop` → overlay put back without redraw

6. Added Ollama result draining in `about_to_wait` (before the PTY poll loop):
   - `try_recv()` on `ollama_result_rx`
   - On `Ok(cmd)` → calls `overlay.set_result(cmd)`
   - On `Err(msg)` → calls `overlay.set_error(msg)`
   - Clears `ollama_result_rx` after consuming

**Deviation:** The existing test `ctrl_space_opens_palette` had to be renamed and updated. This was not a bug — the test correctly described the old behavior. Updating it is the correct action when the binding is intentionally changed.

**Commit:** `0a9746a` — `shipyard(phase-16): wire Ctrl+Space command overlay into keymap and event loop`

---

### Task 3: Render Command Overlay

**File modified:** `arcterm-app/src/main.rs`

Added a rendering block in the `RedrawRequested` render path (before the search overlay block) that activates when `state.command_overlay.is_some()`:

- **Background quad:** `[0.08, 0.09, 0.14, 0.95]` (dark near-black, 95% opacity) at the top of the window; 2.5 cell heights tall (~60px at default scale)
- **Line 1:** `"AI> {query}"` in white at top-left of the bar
- **Line 2:** Phase-dependent text:
  - `OverlayPhase::Input` — empty (cursor blink implied by query line)
  - `OverlayPhase::Loading` — `"  ... waiting for LLM"`
  - `OverlayPhase::Result(cmd)` — `"  >> {cmd}  [Enter to accept · Esc to dismiss]"`
  - `OverlayPhase::Error(msg)` — `"  !! {msg}"`

Pattern follows the established `search_overlay` rendering pattern: push `OverlayQuad` to `overlay_quads` and push text tuples `(String, f32, f32)` to `palette_text`.

**Commit:** `579034c` — `shipyard(phase-16): render command overlay bar with input, loading, and result states`

---

## Verification

All three tasks verified by:

- `cargo test --package arcterm-app` — **347 tests pass, 0 failures** after each task
- `cargo build --package arcterm-app` — **clean build** after Tasks 2 and 3

---

## Final State

| File | Change |
|---|---|
| `arcterm-app/src/command_overlay.rs` | Created (237 lines, 12 tests) |
| `arcterm-app/src/keymap.rs` | Added `OpenCommandOverlay` variant; changed Ctrl+Space mapping; updated test |
| `arcterm-app/src/main.rs` | Registered module; added AppState fields + init; added dispatch; added key routing; added Ollama result drain; added render block |

The complete `Ctrl+Space → type question → Enter → LLM returns command → Enter to accept` flow is wired end-to-end. The overlay renders as a dark bar at the top of the window, transitions through Input → Loading → Result (or Error), and Escape closes it at any phase.
