# SUMMARY-1.1.md — Phase 2, Plan 1.1
## Scrollback Buffer, Scroll Regions, and Grid Mode State

**Date:** 2026-03-15
**Branch:** master
**Commits:** ad0eca8, 677f18f, c1344e0

---

## What Was Done

### Pre-Implementation Reading

Read all relevant source files before writing any code:
- `arcterm-core/src/grid.rs` — Grid struct, all methods, existing tests
- `arcterm-core/src/cell.rs` — Cell, CellAttrs, Color types
- `arcterm-core/src/lib.rs` — current exports
- `arcterm-vt/src/handler.rs` — discovered a fully implemented GridState wrapper with TermModes, Phase 2 methods already in place (not reflected in the plan description)
- `arcterm-vt/src/lib.rs` — comprehensive test suite including Phase 2 processor tests

**Key discovery:** `arcterm-vt/src/handler.rs` already contained a `GridState` wrapper struct, `TermModes`, and full implementations of `insert_lines`, `delete_lines`, `insert_chars`, `delete_chars`, `erase_chars`, alt screen, cursor save/restore, and scroll region support. The plan targeted `arcterm-core` Grid directly, which was correct — this plan adds the same capabilities to the core `Grid` type so consumers do not need the VT layer wrapper.

---

## Task 1: Scrollback Buffer and Scroll Regions

**Commit:** `ad0eca8` — `shipyard(phase-2): add scrollback buffer and scroll regions to grid`

### Tests Written First (TDD)

Five failing tests written before implementation:

| Test | What It Verified |
|---|---|
| `scroll_up_pushes_to_scrollback` | Full-screen scroll_up pushes drained row to scrollback |
| `scrollback_caps_at_max_scrollback` | Scrollback never exceeds max_scrollback |
| `scroll_up_with_region_only_affects_region_rows` | Partial region scroll_up leaves rows outside region unchanged and does not push to scrollback |
| `scroll_down_with_region_only_affects_region_rows` | Partial region scroll_down respects bounds |
| `newline_at_bottom_of_region_scrolls_region_only` | newline_in_region scrolls region, keeps cursor at region bottom, no scrollback |

### Implementation

**New fields on `Grid`:**
- `scrollback: VecDeque<Vec<Cell>>` — rows scrolled off the top (index 0 = most recent)
- `max_scrollback: usize` — cap, default 10,000
- `scroll_region: Option<(usize, usize)>` — (top, bottom) 0-indexed inclusive

**New/modified methods:**
- `scroll_up(n)`: if-let on scroll_region. Partial region: remove/insert rows within bounds, no scrollback. Full screen: drain, push_front to scrollback, pop_back to cap.
- `scroll_down(n)`: if-let on scroll_region. Partial: remove bottom-of-region, insert blank at top. Full screen: truncate + insert at 0.
- `set_scroll_region(top, bottom)`, `clear_scroll_region()` — set/clear field.
- `scrollback_len() -> usize` — public accessor.
- `newline_in_region()` — if at region bottom, scroll_up(1) and stay; else move cursor down.
- `put_char_at_cursor()` — updated to read scroll region bottom when determining wrap-and-scroll boundary.
- `Handler for Grid` `newline()` — updated to delegate to `newline_in_region()`.

**Result:** 45 arcterm-core tests pass (40 original + 5 new). 52 arcterm-vt tests pass (no regressions).

---

## Task 2: Alt Screen, Cursor Save/Restore, Mode Flags, Viewport

**Commit:** `677f18f` — `shipyard(phase-2): add alt screen, cursor save/restore, mode flags, viewport`

### Tests Written First (TDD)

Six failing tests written before implementation:

| Test | What It Verified |
|---|---|
| `term_modes_defaults_correct` | cursor_visible=true, auto_wrap=true, all others false |
| `save_restore_cursor_round_trips` | save then restore recovers cursor pos and CellAttrs |
| `enter_alt_screen_starts_blank` | all cells blank after enter_alt_screen; modes.alt_screen_active=true |
| `leave_alt_screen_restores_original_content` | original cell content present after leave; modes.alt_screen_active=false |
| `rows_for_viewport_at_offset_zero_returns_current_cells` | viewport at offset=0 mirrors live screen |
| `rows_for_viewport_with_scroll_offset_shows_scrollback_mix` | viewport with offset>0 shows scrollback rows then screen rows |

### Implementation

**New type added to `arcterm-core/src/grid.rs`:**

```rust
pub struct TermModes {
    pub cursor_visible: bool,   // default true
    pub auto_wrap: bool,        // default true
    pub app_cursor_keys: bool,  // default false
    pub bracketed_paste: bool,  // default false
    pub alt_screen_active: bool,// default false
    pub app_keypad: bool,       // default false
}
```

**New fields on `Grid`:**
- `modes: TermModes`
- `saved_cursor: Option<(CursorPos, CellAttrs)>`
- `alt_grid: Option<Box<Grid>>`
- `scroll_offset: usize`

**New methods:**
- `save_cursor()` — stores `(cursor, current_attrs)` in `saved_cursor`
- `restore_cursor()` — restores cursor pos and attrs if saved
- `enter_alt_screen()` — saves current cells+cursor+attrs in `alt_grid`, clears display, resets cursor
- `leave_alt_screen()` — restores cells+cursor+attrs from `alt_grid`
- `rows_for_viewport() -> Vec<&Vec<Cell>>` — returns slice of rows composing the visible viewport; at offset=0 returns live cells; at offset>0 prepends scrollback rows in chronological order

**Export:** `TermModes` added to `arcterm-core/src/lib.rs` re-exports.

**Result:** 51 arcterm-core tests pass (45 + 6 new). 65 arcterm-vt tests pass (no regressions).

---

## Task 3: Line and Character Insert/Delete Operations

**Commit:** `c1344e0` — `shipyard(phase-2): add line and character insert/delete operations`

### Implementation

**New methods on `Grid`:**

| Method | Behavior |
|---|---|
| `insert_lines(n)` | Insert n blank rows at cursor row within scroll region; rows at bottom discarded |
| `delete_lines(n)` | Delete n rows at cursor row within scroll region; blank rows appended at bottom |
| `insert_chars(n)` | Insert n blanks at cursor col in cursor row; chars past right edge discarded |
| `delete_chars(n)` | Delete n chars at cursor col; chars shift left; blanks fill right edge |
| `erase_chars(n)` | Overwrite n chars with blanks starting at cursor col; no shifting |

All five methods respect the active `scroll_region` for line operations and clamp to grid bounds.

`TermModes` re-export was already done in Task 2 — no additional lib.rs change needed.

**Result:** 51 arcterm-core tests pass. 70 arcterm-vt tests pass (includes 5 new integration tests that were already present in a pending `lib.rs` edit).

---

## Deviations

1. **Pre-existing `GridState` in `arcterm-vt`:** The plan described adding these capabilities as if they were absent from the codebase. In fact, `arcterm-vt/src/handler.rs` already had `GridState`, `TermModes` (under a different field name `alt_screen` vs `alt_screen_active`), and all Phase 2 methods. The plan was followed as written — capabilities were added to `arcterm-core/src/grid.rs` `Grid` directly. The `GridState` wrapper remains functional and its tests continue to pass.

2. **`lib.rs` had uncommitted new tests:** `arcterm-vt/src/lib.rs` had a new `phase2_integration_tests` module (including `vim_startup_enters_alt_screen_and_sets_scroll_region`) that appeared to have been added between commits. These tests passed after a full rebuild (the initial failure was a stale binary artifact). No code was changed to fix it.

3. **`TermModes.alt_screen_active` vs `.alt_screen`:** The existing `TermModes` in `arcterm-vt/src/handler.rs` uses `alt_screen: bool`. The plan specified `alt_screen_active`. The new `TermModes` in `arcterm-core` uses `alt_screen_active` as specified. The two `TermModes` types coexist independently.

---

## Final State

| Package | Tests | Status |
|---|---|---|
| arcterm-core | 51 | All pass |
| arcterm-vt | 70 | All pass |

**Files modified:**
- `/Users/lgbarn/Personal/myterm/arcterm-core/src/grid.rs`
- `/Users/lgbarn/Personal/myterm/arcterm-core/src/lib.rs`
- `/Users/lgbarn/Personal/myterm/arcterm-vt/src/handler.rs` (newline delegation only)
