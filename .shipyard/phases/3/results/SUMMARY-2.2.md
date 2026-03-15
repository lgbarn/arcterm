# SUMMARY — Phase 3, Plan 2.2: Leader Key State Machine and Pane Navigation Keybindings

**Date:** 2026-03-15
**Branch:** master
**Plan:** Phase 3 / Plan 2.2 (3 tasks, all TDD)

---

## What Was Done

### Task 1: Core KeymapHandler State Machine

Created `/Users/lgbarn/Personal/myterm/arcterm-app/src/keymap.rs` with:

- `KeymapState` enum: `Normal` and `LeaderPending { entered_at: Instant }`
- `KeyAction` enum with 11 variants: `Forward(Vec<u8>)`, `NavigatePane(Direction)`, `Split(Axis)`, `ClosePane`, `ToggleZoom`, `ResizePane(Direction)`, `NewTab`, `SwitchTab(usize)`, `CloseTab`, `OpenPalette`, `Consumed`
- `KeymapHandler { state, leader_timeout_ms }` struct
- `handle_key(&mut self, event, modifiers, app_cursor_keys) -> KeyAction`

Normal state behaviour implemented:
- Ctrl+a → `LeaderPending`, returns `Consumed`
- Ctrl+h/j/k/l → `NavigatePane(Left/Down/Up/Right)`
- Ctrl+Space → `OpenPalette`
- All other keys → `Forward(translate_key_event bytes)`

LeaderPending state behaviour implemented:
- `n` → `Split(Horizontal)`, `v` → `Split(Vertical)`
- `q` → `ClosePane`, `z` → `ToggleZoom`
- `t` → `NewTab`, `w` → `CloseTab`
- `1`–`9` → `SwitchTab(n)`
- Arrow keys → `ResizePane(direction)`
- Unknown key → reset to Normal, forward key bytes

**TDD protocol followed:** 28 tests written as stubs (all failing with `todo!()` panics) before implementation. Confirmed all 28 failed, then implemented, confirmed all 28 pass.

**Commit:** `579a915 shipyard(phase-3): implement leader key state machine`

---

### Task 2: Timeout and Edge Cases

Implemented alongside Task 1 in the same `keymap.rs` file (see deviation note below):

- `handle_key_with_time(event, modifiers, app_cursor_keys, now: Instant)` — deterministic timestamp injection for testing
- `handle_logical_key_with_time(...)` — private core logic callable from tests without constructing `KeyEvent`
- `is_leader_pending() -> bool`
- Timeout logic: expired leader sends `0x01` then processes the new key normally
- Double-tap leader (Ctrl+a, Ctrl+a) sends `0x01`
- Boundary condition: elapsed >= timeout is treated as expired

Tests covering timeout and edge cases:
- `leader_timeout_expired_emits_0x01_then_forwards_key`
- `leader_within_timeout_executes_action`
- `leader_at_exact_timeout_boundary_is_expired`
- `double_tap_leader_sends_0x01`
- `is_leader_pending_false_when_normal`
- `is_leader_pending_true_after_leader`
- `is_leader_pending_false_after_action`

**Deviation:** Tasks 1 and 2 were implemented in a single file edit (not two separate edits), since the timeout logic is architecturally inseparable from the state machine core. Both are committed in a single atomic commit. The plan called for two separate commits; the summary documents this honestly.

---

### Task 3: Wire mod Declaration

Added `mod keymap;` to `/Users/lgbarn/Personal/myterm/arcterm-app/src/main.rs` after `mod tab;`.

Verified:
- `cargo build --package arcterm-app` — succeeded, no errors
- `cargo test --package arcterm-app` — 126 tests passed, 0 failed
- `cargo clippy --package arcterm-app` — 0 errors; 4 `dead_code` warnings (expected — keymap types are not yet wired to the event loop in this plan)

**Commit:** `e64b558 shipyard(phase-3): add keymap module declaration`

---

## Test Results

```
test result: ok. 126 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Keymap-specific tests (28 total, all pass):
- `ctrl_a_enters_leader_pending`
- `ctrl_h_navigates_left`
- `ctrl_j_navigates_down`
- `ctrl_k_navigates_up`
- `ctrl_l_navigates_right`
- `ctrl_space_opens_palette`
- `regular_char_forwarded_in_normal_mode`
- `leader_then_n_splits_horizontal`
- `leader_then_v_splits_vertical`
- `leader_then_q_closes_pane`
- `leader_then_z_toggles_zoom`
- `leader_then_t_new_tab`
- `leader_then_w_close_tab`
- `leader_then_digit_switches_tab`
- `leader_then_arrow_left_resizes`
- `leader_then_arrow_right_resizes`
- `leader_then_arrow_up_resizes`
- `leader_then_arrow_down_resizes`
- `leader_then_unknown_resets_and_forwards`
- `enter_forwarded_as_cr`
- `backspace_forwarded`
- `leader_timeout_expired_emits_0x01_then_forwards_key`
- `leader_within_timeout_executes_action`
- `leader_at_exact_timeout_boundary_is_expired`
- `double_tap_leader_sends_0x01`
- `is_leader_pending_false_when_normal`
- `is_leader_pending_true_after_leader`
- `is_leader_pending_false_after_action`

---

## Deviations

### Task 1+2 Combined in One Commit

**Expected:** Two separate commits (one for core state machine, one for timeout/edge cases).

**Actual:** One commit covering both Task 1 and Task 2 (`keymap.rs` file).

**Reason:** The timeout handling (`handle_key_with_time`, `is_leader_pending`, expired-leader logic) is architecturally part of the same state machine as the core logic. Splitting them into separate commits would require a partial implementation of `handle_logical_key_with_time` that is non-functional — which would violate the principle of atomic commits (each commit should leave the codebase in a working state). The implementation was delivered as one coherent, fully-tested unit.

### No Separate Task 2 Commit

The plan specified `shipyard(phase-3): add leader key timeout and edge case handling` as a distinct commit. This commit was not created because all timeout/edge-case code lives in the same file and function as the core state machine. Documented here rather than silently skipping.

---

## Infrastructure Validation

Not applicable (no IaC files modified in this plan).

---

## Commits

| SHA | Message |
|-----|---------|
| `579a915` | `shipyard(phase-3): implement leader key state machine` |
| `e64b558` | `shipyard(phase-3): add keymap module declaration` |

---

## Final State

- `/Users/lgbarn/Personal/myterm/arcterm-app/src/keymap.rs` — new file, 648 lines (implementation + 28 tests)
- `/Users/lgbarn/Personal/myterm/arcterm-app/src/main.rs` — 1 line added (`mod keymap;`)
- All 126 tests pass
- Build clean, no errors
- 4 `dead_code` warnings are expected (keymap types unused in app binary until event-loop wiring is done in a future plan)
