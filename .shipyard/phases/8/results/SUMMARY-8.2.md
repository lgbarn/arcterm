# SUMMARY-8.2: Cross-Pane Regex Search (Leader+/)

**Plan:** PLAN-8.2
**Phase:** 8 — Config, Overlays, Polish & Release
**Status:** Complete

---

## Tasks Executed

### Task 1: Grid text extraction + SearchOverlayState (TDD)

**Status:** Done — all tests pass

**Grid additions (`arcterm-core/src/grid.rs`):**

- `pub fn row_to_string(row: &[Cell]) -> String` — collects each cell's `c` char into a String; spaces for blank cells.
- `pub fn all_text_rows(&self) -> Vec<String>` — scrollback rows oldest-first (iterating `self.scrollback` in reverse, since index 0 is most-recent) followed by all visible rows. Produces a contiguous top-to-bottom text representation of the full terminal content.

Four tests added and passing:
- `row_to_string_produces_correct_string`
- `row_to_string_includes_spaces_for_blank_cells`
- `all_text_rows_returns_scrollback_before_visible`
- `all_text_rows_multiple_scrollback_rows_oldest_first`

**`arcterm-app/src/search.rs` (new file, 388 lines):**

`SearchMatch` struct: `pane_id`, `row_index`, `col_start`, `col_end` (all character-indexed, not byte-indexed).

`SearchOverlayState` methods:
- `new()` — empty state
- `update_query(String)` — compiles regex, sets `error_msg` on failure
- `execute_search(&[(PaneId, Vec<String>)])` — finds all matches via `Regex::find_iter`, maps byte offsets to character indices via `byte_offset_to_char_index` helper
- `next_match()` / `prev_match()` — modular index arithmetic with wraparound
- `current()` — returns current `SearchMatch` reference
- `handle_key(&Key)` — maps Enter→Execute, Esc→Close, Backspace→UpdateQuery, n→NextMatch (when compiled), N→PrevMatch (when compiled), printable→UpdateQuery
- `should_auto_search()` — debounce: true when `compiled.is_some()` and >200ms since `last_query_change`
- `match_quads_for_pane(...)` — computes pixel quads for matches visible in viewport, using all_text_rows-space row index math
- `scroll_offset_for_match(...)` — centers match row in viewport using live-start arithmetic

Nineteen tests added and passing covering: regex compilation, multi-pane search, navigation wraparound, UTF-8 byte-to-column mapping, debounce timing, quad pixel rect correctness, viewport filtering, scrollback offset calculation, empty query edge case, no-results state.

**Commit:** `3450467` — `shipyard(phase-8): add Grid text extraction and SearchOverlayState (Task 1)`

---

### Task 2: Wire search into AppState + rendering (no TDD)

**Status:** Done — `cargo build --package arcterm-app` succeeds

**`arcterm-app/src/main.rs` additions:**

- `mod search;` declaration added to module list.
- `search_overlay: Option<search::SearchOverlayState>` field added to `AppState` struct, initialized to `None`.
- `KeyAction::CrossPaneSearch` arm added to the main keyboard match (opens `SearchOverlayState::new()`).
- `KeyAction::ReviewOverlay` arm added as no-op (already existed from Plan 8.1; was missing in one match arm).
- Both variants also added to the `execute_key_action` exhaustive no-op arm.
- Modal routing block inserted before the keymap handler: when `search_overlay.is_some()`, all keyboard events route to `overlay.handle_key()` and dispatch on `SearchAction`.
  - `NextMatch` / `PrevMatch` auto-scroll the focused pane's `grid.scroll_offset` via `scroll_offset_for_match`.
  - `Execute` collects `all_text_rows()` from all panes and calls `execute_search`.
- Rendering added before plugin pane collection: top-anchored input bar (semi-transparent, `cell_h * 1.5` tall) with query/match-count text; match highlight quads generated via `match_quads_for_pane` for each visible pane.

**Commit:** `d0f3df6` — `shipyard(phase-8): wire search overlay into AppState and rendering (Task 2)`

---

### Task 3: Edge cases (TDD)

**Status:** Done — all 19 tests pass

All edge-case functionality was implemented in Task 1 (the `search.rs` file was written with the full feature set including edge cases from the start). The `about_to_wait` debounce wiring was added as part of the Task 2 commit:

- Debounced auto-search in `about_to_wait`: checks `should_auto_search()` and calls `execute_search` automatically without requiring Enter.
- `match_quads_for_pane` — correctly skips rows outside the viewport and returns `(OverlayQuad, bool)` pairs.
- `scroll_offset_for_match` — pure function: centers the match row in the viewport.

All 6 Task 3 specified test cases are present and passing under `search::tests`:
- `should_auto_search_false_immediately_after_query_change`
- `should_auto_search_true_after_debounce_elapsed`
- `match_quads_for_pane_empty_when_no_matches_in_pane`
- `match_quads_for_pane_correct_pixel_rects`
- `scroll_offset_for_match_centers_deep_scrollback_row`
- `scroll_offset_for_match_visible_row_returns_zero`
- (Plus empty query and no-results tests)

---

## Files Touched

| File | Change |
|------|--------|
| `arcterm-core/src/grid.rs` | Added `row_to_string`, `all_text_rows`, 4 tests |
| `arcterm-app/src/search.rs` | New file: full `SearchOverlayState` implementation, 19 tests |
| `arcterm-app/src/main.rs` | `mod search`, `search_overlay` field, modal routing, rendering, debounce |

## Verification Results

```
cargo test --package arcterm-core -- row_to_string all_text_rows
# 4 passed; 0 failed

cargo test --package arcterm-app -- search::tests
# 19 passed; 0 failed

cargo build --package arcterm-app
# Finished dev profile — 0 errors, warnings only
```

## Deviations

1. **Task 3 tests written upfront with Task 1:** The plan specified Task 3 as a separate TDD wave. Since `match_quads_for_pane`, `scroll_offset_for_match`, and `should_auto_search` were part of `SearchOverlayState` in `search.rs`, their tests were included in the same file during Task 1 implementation. This avoids fragmented commits and keeps all search module tests in one place. All specified test cases are present and verified.

2. **Debounce wiring committed in Task 2:** The `about_to_wait` debounce block was added to `main.rs` before the Task 2 commit, so it was captured there rather than in a separate Task 3 commit. The functionality is complete and tested.

3. **ReviewOverlay no-op arms added:** Plan 8.2 only specifies `CrossPaneSearch`, but Plan 8.1 had already added `ReviewOverlay` to `KeyAction`. Both variants needed exhaustive match coverage to compile. Added as no-ops where not already handled.
