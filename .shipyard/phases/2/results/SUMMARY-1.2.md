# SUMMARY-1.2.md — Plan 1.2: DEC Private Modes and Extended VT Sequences

**Phase:** 2 — Terminal Fidelity and Configuration
**Plan:** 1.2
**Branch:** master
**Date:** 2026-03-15
**Final test count:** 70 passed, 0 failed

---

## Task 1: Extend Handler Trait + Grid Implementations

**Status:** Complete
**Commit:** `0f72684` — `shipyard(phase-2): extend handler trait with DEC mode and editing methods`

### What was done

Added 15 new methods to the `Handler` trait in `arcterm-vt/src/handler.rs`, all with default no-op implementations:

- `set_mode(mode, private)` / `reset_mode(mode, private)` — DEC private and standard mode control
- `set_scroll_region(top, bottom)` — DECSTBM scroll region
- `save_cursor_position()` / `restore_cursor_position()` — DECSC/DECRC
- `insert_lines(n)` / `delete_lines(n)` — IL/DL line editing
- `insert_chars(n)` / `delete_chars(n)` / `erase_chars(n)` — ICH/DCH/ECH character editing
- `cursor_horizontal_absolute(col)` / `cursor_vertical_absolute(row)` — CHA/VPA
- `device_status_report(n)` / `device_attributes()` — DSR/DA (no-op, no PTY write-back)
- `set_keypad_application_mode()` / `set_keypad_numeric_mode()` — DECKPAM/DECKPNM

### TermModes struct

Added `TermModes` with fields: `cursor_visible`, `auto_wrap`, `app_cursor_keys`, `alt_screen`, `bracketed_paste`, `app_keypad`. Defaults match standard terminal startup (cursor visible, auto-wrap on, others off).

### GridState wrapper

Added `GridState` struct in handler.rs to carry Phase 2 state that does not yet exist in `arcterm-core::Grid` (since Plan 1.1 is building those concurrently):

- `grid: Grid` — the underlying core grid
- `modes: TermModes` — active mode flags
- `scroll_top` / `scroll_bottom` — scroll region boundaries (0-indexed)
- `saved_cursor: Option<CursorPos>` — DECSC saved position
- `normal_screen: Option<Grid>` — normal screen saved on alt screen entry

Full `Handler` implementation for `GridState` with region-aware scroll helpers (`scroll_region_up`, `scroll_region_down`) that operate only within the configured scroll region.

The legacy `impl Handler for Grid` was preserved unchanged for backward compatibility with all Phase 1 tests.

### Deviation

The plan said "delegate insert/delete/erase to Grid methods" but those Grid methods don't exist yet (they are being added by Plan 1.1 in arcterm-core). To keep the code compiling independently, the logic was implemented inline in the `GridState` Handler impl using direct `self.grid.cells` access. This is safe and will not conflict when Plan 1.1 merges — the `GridState` impl is self-contained in arcterm-vt.

---

## Task 2: Processor CSI + ESC Dispatch (TDD)

**Status:** Complete
**Commit:** `1989bdd` — `shipyard(phase-2): add DEC private mode and extended CSI/ESC dispatch`

### TDD: Tests written first, confirmed failing (12 of 13 failed)

The `decstbm_no_params_resets_to_full_screen` test passed immediately because the GridState defaults already represent a full-screen scroll region — this is correct behavior, not a pre-implementation pass.

### New CSI arms in `csi_dispatch`

| Sequence | Action |
|----------|--------|
| `ESC[?...h` / `ESC[?...l` | DEC private mode set/reset (detected via `intermediates.contains(&0x3F)`) |
| `ESC[...h` / `ESC[...l` | Standard mode set/reset |
| `ESC[Lr` | DECSTBM — scroll region (usize::MAX sentinel for default bottom) |
| `ESC[nL` | IL — insert lines |
| `ESC[nM` | DL — delete lines |
| `ESC[n@` | ICH — insert characters |
| `ESC[nP` | DCH — delete characters |
| `ESC[nX` | ECH — erase characters |
| `ESC[nG` | CHA — cursor horizontal absolute |
| `ESC[nd` | VPA — cursor vertical absolute |
| `ESC[nn` | DSR — device status report |
| `ESC[nc` | DA — device attributes |

### New `esc_dispatch` handler

| Byte | Sequence | Action |
|------|----------|--------|
| `0x37` (`7`) | ESC 7 | `save_cursor_position` |
| `0x38` (`8`) | ESC 8 | `restore_cursor_position` |
| `0x3D` (`=`) | ESC = | `set_keypad_application_mode` |
| `0x3E` (`>`) | ESC > | `set_keypad_numeric_mode` |

### Multi-mode dispatch

The `h`/`l` arms iterate all parameter groups, enabling sequences like `ESC[?1;25;2004h` to set multiple modes at once.

### DECSTBM sentinel

The `r` arm passes `usize::MAX` as the bottom row when the parameter is absent or 0, and `set_scroll_region` in handler.rs maps `usize::MAX` to `max_row`. This cleanly handles `ESC[r` (full-screen reset) without needing to know grid dimensions in the processor.

---

## Task 3: Integration Tests (TDD)

**Status:** Complete
**Commit:** `dcbef63` — `shipyard(phase-2): add VT integration tests for vim/htop scenarios`

### Tests added (`phase2_integration_tests`)

| Test | Scenario |
|------|----------|
| `vim_startup_enters_alt_screen_and_sets_scroll_region` | Full vim startup: `ESC[?1049h` + `ESC[2J` + `ESC[1;1H` + `ESC[1;24r` + text |
| `vim_exit_restores_normal_screen` | `ESC[r` + `ESC[?1049l` restores saved grid |
| `htop_scroll_region_excludes_last_row` | Status bar (row 23) untouched after LF at region bottom |
| `multi_mode_set_sets_all_modes` | `ESC[?1;25;2004h` sets app_cursor_keys, cursor_visible, bracketed_paste |
| `scroll_up_respects_scroll_region_boundaries` | `CSI S` only scrolls within configured region |

### TDD note

The `vim_startup` test initially failed because the cursor assertion `== (0,0)` was written after "Hello" was fed, advancing the cursor to `(0,5)`. The test was corrected to split the sequence: assert cursor home after DECSTBM, then feed text and assert character placement and final cursor position separately. The test was confirmed failing before being corrected and verified.

---

## Files Modified

- `/Users/lgbarn/Personal/myterm/arcterm-vt/src/handler.rs` — Handler trait extensions, TermModes, GridState, Handler impls
- `/Users/lgbarn/Personal/myterm/arcterm-vt/src/processor.rs` — Extended csi_dispatch and esc_dispatch
- `/Users/lgbarn/Personal/myterm/arcterm-vt/src/lib.rs` — Export GridState/TermModes; phase2_processor_tests and phase2_integration_tests modules

## Files NOT Modified (as required)

- `arcterm-core/src/grid.rs` — Not touched; all Phase 2 state lives in arcterm-vt's GridState

---

## Final State

```
cargo test --package arcterm-vt
test result: ok. 70 passed; 0 failed; 0 ignored
```

Baseline was 52 tests. This plan added 18 new tests (13 in phase2_processor_tests, 5 in phase2_integration_tests).
