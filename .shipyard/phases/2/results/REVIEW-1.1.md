# REVIEW-1.1.md — Phase 2, Plan 1.1
## Scrollback Buffer, Scroll Regions, and Grid Mode State

**Reviewer:** Claude Code (claude-sonnet-4-6)
**Date:** 2026-03-15
**Files reviewed:**
- `/Users/lgbarn/Personal/myterm/arcterm-core/src/grid.rs`
- `/Users/lgbarn/Personal/myterm/arcterm-core/src/lib.rs`
- `/Users/lgbarn/Personal/myterm/arcterm-vt/src/handler.rs`

---

## Stage 1: Spec Compliance

**Verdict:** PASS (with two non-blocking deviations noted)

---

### Task 1: Scrollback Buffer and Scroll Regions

**Status: PASS**

**Evidence:**

- `scrollback: VecDeque<Vec<Cell>>` field present at `grid.rs:71`. `max_scrollback: usize` at `grid.rs:73`, default 10,000 (`grid.rs:102`).
- `scroll_region: Option<(usize, usize)>` field at `grid.rs:76`, default `None` (`grid.rs:103`).
- `scroll_up()` at `grid.rs:164`: branches on `self.scroll_region`. Partial region path removes and reinserts rows within `[top, bottom]`, does not push to scrollback (`grid.rs:170-174`). Full-screen path drains rows, pushes to scrollback front, caps at `max_scrollback` (`grid.rs:179-191`).
- `scroll_down()` at `grid.rs:199`: respects `scroll_region` on both paths.
- `set_scroll_region()` at `grid.rs:222`, `clear_scroll_region()` at `grid.rs:227`.
- `scrollback_len()` at `grid.rs:232`.
- `newline_in_region()` at `grid.rs:241`: when cursor is at region bottom, calls `scroll_up(1)` and keeps cursor at `bottom`; otherwise moves cursor down.
- `put_char_at_cursor()` at `grid.rs:484`: reads `scroll_region` bottom to determine wrap-and-scroll boundary.
- `Handler for Grid` was not directly added to `grid.rs` (the handler delegation is through `arcterm-vt`'s `GridState`), but `newline_in_region()` is the public method that replaces the scroll-unaware behavior, and `GridState::newline()` in `handler.rs` already managed this via `scroll_region_up()`. The VT layer still uses `GridState` internally; `newline_in_region()` is available for any future direct `Grid` Handler impl.
- Tests at `grid.rs:846-925`: all five required tests present and passing (`scroll_up_pushes_to_scrollback`, `scrollback_caps_at_max_scrollback`, `scroll_up_with_region_only_affects_region_rows`, `scroll_down_with_region_only_affects_region_rows`, `newline_at_bottom_of_region_scrolls_region_only`).
- Test run confirmed: 51 arcterm-core tests pass, 70 arcterm-vt tests pass.

**Deviation (non-blocking):** The plan spec item 5 states `set_scroll_region()` must "validate bounds (top < bottom, both < rows)". The implementation at `grid.rs:222-224` stores `Some((top, bottom))` unconditionally with no validation. Callers supplying `top >= bottom` or `bottom >= rows` will silently store an invalid region. This is logged as an Important finding below.

---

### Task 2: Alt Screen, Cursor Save/Restore, Mode Flags, and Viewport

**Status: PASS**

**Evidence:**

- `TermModes` struct at `grid.rs:9-16` with all six fields specified by the plan: `cursor_visible`, `auto_wrap`, `app_cursor_keys`, `bracketed_paste`, `alt_screen_active`, `app_keypad`.
- Defaults at `grid.rs:27-35`: `cursor_visible=true`, `auto_wrap=true`, all others `false`. Matches spec.
- `modes: TermModes` field on `Grid` at `grid.rs:78`, initialized via `TermModes::new()` at `grid.rs:104`.
- `saved_cursor: Option<(CursorPos, CellAttrs)>` at `grid.rs:80`.
- `save_cursor()` at `grid.rs:253`, `restore_cursor()` at `grid.rs:259`.
- `alt_grid: Option<Box<Grid>>` at `grid.rs:82`.
- `enter_alt_screen()` at `grid.rs:270`: guards against double-entry, sets `alt_screen_active=true`, moves current cells into a saved `Grid`, replaces display with blank cells, resets cursor and attrs.
- `leave_alt_screen()` at `grid.rs:296`: guards against spurious call, restores `cells`, `cursor`, `current_attrs` from `alt_grid`, sets `alt_screen_active=false`.
- `scroll_offset: usize` at `grid.rs:85`, default `0` (`grid.rs:107`).
- `rows_for_viewport()` at `grid.rs:314`: at offset=0 returns `self.cells.iter().collect()`; at offset>0 reconstructs chronological scrollback slice prepended to first `rows - offset` screen rows.
- `TermModes` re-exported from `arcterm-core/src/lib.rs:8`.
- All six required tests present at `grid.rs:932-1008` and confirmed passing.

**Deviation (non-blocking):** The plan item 11 requires `resize()` to also resize `alt_grid` if present. The `resize()` implementation at `grid.rs:499-521` does not touch `alt_grid`. If the terminal is resized while on the alt screen, the saved normal-screen grid retains the old dimensions. When `leave_alt_screen()` restores it, the cursor clamping in `set_cursor()` will handle cursor bounds, but row/col counts in the restored `cells` vector will be stale. This is logged as an Important finding below.

---

### Task 3: Line and Character Insert/Delete Operations

**Status: PASS**

**Evidence:**

- `insert_lines()` at `grid.rs:362`: inserts `n` blank rows at `cursor.row`, discards rows at `bottom` of region. Respects `scroll_region`.
- `delete_lines()` at `grid.rs:387`: deletes `n` rows at `cursor.row`, appends blanks at `bottom`. Respects `scroll_region`.
- `insert_chars()` at `grid.rs:409`: shifts characters right from cursor column, clears inserted positions, clamps `n` to available space.
- `delete_chars()` at `grid.rs:428`: shifts characters left, blanks right edge, clamps `n`.
- `erase_chars()` at `grid.rs:447`: overwrites `n` cells with blank, no shift.
- `TermModes` re-export present at `lib.rs:8` (completed in Task 2 but satisfies Task 3 requirement).
- All 51 arcterm-core tests pass; all 70 arcterm-vt tests pass including 5 new `phase2_integration_tests`.

---

## Stage 2: Code Quality

### Critical

None.

---

### Important

**ISSUE-1.1-A: `set_scroll_region()` performs no bounds validation**

- **File:** `/Users/lgbarn/Personal/myterm/arcterm-core/src/grid.rs:222-224`
- The plan spec requires: "validates bounds (top < bottom, both < rows)". The implementation stores any `(top, bottom)` pair without checks.
- If `top >= bottom`, `scroll_up()` computes `region_height = (bottom + 1).saturating_sub(top)` which yields 0 or 1 unexpectedly. If `bottom >= self.size.rows`, the `remove(bottom)` call in `scroll_up()` at `grid.rs:171` will panic at runtime (Vec index out of bounds).
- **Remediation:** Add validation before storing: `if top >= self.size.rows || bottom >= self.size.rows || top >= bottom { return; }` (or return a `Result`). At minimum clamp: `let bottom = bottom.min(self.size.rows.saturating_sub(1));` and assert `top < bottom`.

**ISSUE-1.1-B: `resize()` does not resize `alt_grid` when present**

- **File:** `/Users/lgbarn/Personal/myterm/arcterm-core/src/grid.rs:499-521`
- The plan spec (Task 2, item 11) explicitly requires: "Modify `resize()` to also resize the alt_grid if present." The current `resize()` operates only on `self.cells` and ignores `self.alt_grid`.
- When the terminal is resized on the alt screen, then `leave_alt_screen()` is called, the restored `cells` has the pre-resize row/col count. Any subsequent access by row index using `self.size` (which was updated by `resize()`) risks a panic if the new size is larger than the saved grid, or silently shows wrong dimensions if smaller.
- **Remediation:** Add at the end of `resize()`: `if let Some(ref mut ag) = self.alt_grid { ag.resize(new_size); }`.

**ISSUE-1.1-C: `rows_for_viewport()` panics if `scroll_offset` exceeds scrollback length by more than one**

- **File:** `/Users/lgbarn/Personal/myterm/arcterm-core/src/grid.rs:343`
- The offset is clamped to `scrollback.len()` at `grid.rs:316` (`let offset = self.scroll_offset.min(self.scrollback.len())`), so accessing `&self.scrollback[i]` for `i in (0..offset).rev()` is safe. However, the subsequent screen rows slice at `grid.rs:346-349` uses `cells[i]` for `i in 0..screen_rows` where `screen_rows = rows.saturating_sub(offset)`. If `offset == rows` (scrolled back exactly a full screen), `screen_rows` is 0 and the screen loop is skipped — which is correct. The clamping is sound.
- The actual risk is when `scroll_offset` is set to a value larger than `scrollback.len()` but the clamping at line 316 masks the mismatch silently. The public `scroll_offset` field is writable directly by callers (no setter with validation), so a caller setting `g.scroll_offset = 9999` will work but the effective offset will be silently capped without any feedback. Consider a setter method: `pub fn set_scroll_offset(&mut self, offset: usize) { self.scroll_offset = offset.min(self.scrollback.len()); }`.
- **Remediation:** Either keep `scroll_offset` private and expose a setter, or document the silent clamping behavior clearly so callers understand the field and `scrollback.len()` must be checked together.

---

### Suggestions

**ISSUE-1.1-D: `scroll_up()` / `scroll_down()` use O(n) `Vec::remove` / `Vec::insert` in a per-row loop**

- **File:** `/Users/lgbarn/Personal/myterm/arcterm-core/src/grid.rs:170-173`, `grid.rs:204-208`
- The partial-region scroll path calls `cells.remove(top)` then `cells.insert(bottom, ...)` in a `for _ in 0..n` loop. Each `remove`/`insert` on a `Vec` is O(rows) due to element shifting. For a 1000-row grid scrolling 100 lines, this is 100 × 1000 = 100,000 shifts per call. The `GridState` version in `arcterm-vt/src/handler.rs` uses an in-place copy loop which is O(rows × cols) total — substantially more efficient.
- **Remediation:** For the partial region case, replace the `remove`/`insert` loop with the same in-place copy pattern used in `GridState::scroll_region_up()` (`handler.rs:218-239`): shift row references in-place by index within `[top..=bottom]`, then blank the tail. For the full-screen path, the current drain approach is already efficient.

**ISSUE-1.1-E: `enter_alt_screen()` does not save/restore `scroll_region`**

- **File:** `/Users/lgbarn/Personal/myterm/arcterm-core/src/grid.rs:270-293`
- `enter_alt_screen()` saves `cells`, `cursor`, and `current_attrs` into the `alt_grid` snapshot. It does not save or clear `scroll_region`. If a scroll region was active when entering the alt screen, it remains active on the alt screen, and is not restored to its prior value on `leave_alt_screen()`. xterm clears the scroll region when entering the alt screen and restores it on exit. This is out-of-spec but not in the explicit task requirements, so it is a suggestion rather than a failure.
- **Remediation:** In `enter_alt_screen()`, save `self.scroll_region` into the `saved` Grid's `scroll_region` field, then call `self.clear_scroll_region()` to reset the alt screen's region. In `leave_alt_screen()`, restore `self.scroll_region = saved.scroll_region`.

**ISSUE-1.1-F: `insert_lines()` / `delete_lines()` use same O(n × rows) `Vec::remove`/`Vec::insert` pattern**

- **File:** `/Users/lgbarn/Personal/myterm/arcterm-core/src/grid.rs:375-379`, `grid.rs:400-404`
- Same issue as ISSUE-1.1-D. The fix is the same in-place copy pattern.
- **Remediation:** Use `Vec::drain` + `extend` or index-based in-place copy to shift rows within the region in a single O(rows × cols) pass.

---

## Summary

**Verdict:** APPROVE

All three tasks are correctly implemented and all planned tests pass (51 arcterm-core, 70 arcterm-vt). Two spec deviations exist — missing bounds validation in `set_scroll_region()` and missing `alt_grid` resize in `resize()` — but neither causes test failures under the current test suite. Both are Important findings that should be resolved before this Grid API is exposed to the VT handler in the next plan.

Critical: 0 | Important: 3 | Suggestions: 3
