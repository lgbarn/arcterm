# SUMMARY-1.2 — VT Regression Tests (PLAN-1.2)

## Status: COMPLETE

## Commit

`57ff87b` — `shipyard(phase-9): add VT regression tests for ISSUE-011, 012, 013`

## Tasks Executed

### Task 1 — ISSUE-011: esc_dispatch intermediates guard

**File:** `arcterm-vt/src/processor.rs`

Added two tests in `mod phase9_regression_tests`:

- `esc_dispatch_with_intermediates_does_not_save_cursor` — sends `ESC ( 7` (SCS sequence with intermediate `(`) and asserts `gs.saved_cursor.is_none()`, proving the intermediates guard prevents mis-dispatch of save_cursor_position.
- `esc_dispatch_bare_esc7_saves_cursor` — sends bare `ESC 7` (DECSC), moves cursor, sends `ESC 8` (DECRC), asserts cursor is restored. Positive regression for the working path.

**Deviation from plan:** The plan asserted `gs.grid.cursor()` position for the negative test, which would always pass (save_cursor doesn't move the cursor). Used `gs.saved_cursor.is_none()` instead — a meaningful assertion proving the cursor was NOT saved.

**Verify:** `cargo test -p arcterm-vt -- esc_dispatch` → 2 passed ✓

### Task 2 — ISSUE-012: modes 47/1047 and mouse modes 1000/1006

**File:** `arcterm-vt/src/processor.rs`

Added five tests in `mod phase9_regression_tests`:

- `set_mode_47_enters_alt_screen` — CSI `?47h` sets `gs.modes.alt_screen = true`
- `reset_mode_47_leaves_alt_screen` — CSI `?47l` clears `gs.modes.alt_screen`
- `set_mode_1000_enables_mouse_click_report` — CSI `?1000h` sets `gs.modes.mouse_report_click`
- `reset_mode_1000_disables_mouse_click_report` — CSI `?1000l` clears `gs.modes.mouse_report_click`
- `set_mode_1006_enables_sgr_mouse_ext` — CSI `?1006h` sets `gs.modes.mouse_sgr_ext`

**Deviation from plan:** The plan referenced `gs.grid.alt_grid.is_some()` which does not exist. The actual implementation uses `gs.modes.alt_screen` (bool on GridState). Adapted assertions accordingly.

**Verify:** `cargo test -p arcterm-vt -- set_mode && cargo test -p arcterm-vt -- reset_mode` → 5+2 passed ✓

### Task 3 — ISSUE-013: newline cursor-above-scroll-region behavior

**File:** `arcterm-vt/src/processor.rs`

Added one test in `mod phase9_regression_tests`:

- `newline_cursor_above_scroll_region_advances_into_region` — 10-row grid, scroll region rows 3–7 (0-indexed, set via `CSI 4;8 r`). Cursor placed at row 0, then 8 newlines issued. Asserts:
  - Rows 0→1→2→3: advance freely (cursor above region)
  - Rows 3→4→5→6→7: advance within region
  - Row 7 + newline: region scrolls, cursor stays pinned at 7

Added `make_gs_with_size(rows, cols)` helper (plan required it, did not previously exist).

**Verify:** `cargo test -p arcterm-vt -- newline_cursor_above_scroll_region` → 1 passed ✓

## Final Verification

```
cargo test -p arcterm-vt && cargo clippy -p arcterm-vt -- -D warnings
```

**157 tests passed. Clippy: clean (no warnings).**

## New Test Module

All 8 regression tests live in `processor::phase9_regression_tests` in `arcterm-vt/src/processor.rs`.
