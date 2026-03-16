# REVIEW-1.1 — Grid Fixes (arcterm-core)

**Plan:** PLAN-1.1
**Phase:** 9
**Reviewer:** shipyard:reviewer
**Commits reviewed:** `9abfcd4`, `1996db9`, `a243f8a`
**File reviewed:** `arcterm-core/src/grid.rs`

---

## Stage 1: Spec Compliance

**Verdict:** PASS

### Task 1: ISSUE-007 set_scroll_region bounds validation + ISSUE-008 alt_grid resize

- **Status:** PASS
- **Evidence (ISSUE-007):** Guard added at `grid.rs:248–250`:
  ```rust
  if top >= self.size.rows || bottom >= self.size.rows || top >= bottom {
      return;
  }
  ```
  Matches the plan spec exactly. Three required tests present and correctly structured:
  - `set_scroll_region_rejects_inverted_bounds` (top=5, bottom=2 → assert `scroll_region == None`) ✓
  - `set_scroll_region_rejects_bottom_out_of_range` (bottom=rows → None) ✓
  - `set_scroll_region_rejects_top_out_of_range` (top=rows → None) ✓
- **Evidence (ISSUE-008):** Appended to `resize()` at `grid.rs:571–573`:
  ```rust
  if let Some(ref mut ag) = self.alt_grid {
      ag.resize(new_size);
  }
  ```
  Matches the plan spec exactly. Required test present:
  - `resize_also_resizes_alt_grid`: creates 5×10 grid, enters alt screen, resizes to 8×20, leaves alt screen, asserts `cells.len() == 8` and `cells[0].len() == 20` ✓
- **Notes:** The summary correctly counts 4 tests added for Task 1 (3 for ISSUE-007 + 1 for ISSUE-008). No out-of-scope changes observed.

### Task 2: ISSUE-009 scroll_offset encapsulation

- **Status:** PASS
- **Evidence:** `pub scroll_offset: usize` changed to `scroll_offset: usize` at `grid.rs:92` (confirmed in diff). Two public methods added at `grid.rs:265–272`:
  ```rust
  pub fn set_scroll_offset(&mut self, offset: usize) {
      self.scroll_offset = offset.min(self.scrollback.len());
  }
  pub fn scroll_offset(&self) -> usize {
      self.scroll_offset
  }
  ```
  Both match the plan spec exactly. Existing test updated: `g.scroll_offset = 1` → `g.set_scroll_offset(1)` at `grid.rs:1115`. Two new tests present:
  - `set_scroll_offset_clamps_to_scrollback_len`: 5 scroll_up calls (5 scrollback rows), `set_scroll_offset(100)` → assert `scroll_offset() == 5` ✓
  - `set_scroll_offset_zero_is_valid`: `set_scroll_offset(0)` → assert `scroll_offset() == 0` ✓
- **Notes:** Cross-crate breakage in `arcterm-app` acknowledged in SUMMARY-1.1 and matches the plan's explicit warning. Verified only against `arcterm-core` per spec.

### Task 3: ISSUE-010 in-place scroll operations

- **Status:** PASS
- **Evidence:** All four Vec::remove/Vec::insert loops replaced with in-place index-copy patterns:
  - `scroll_up` partial-region (`grid.rs:181–192`): forward copy `top..=(bottom-n)`, blank tail `(bottom+1-n)..=bottom` ✓
  - `scroll_down` partial-region (`grid.rs:222–232`): reverse copy `(top+n..=bottom).rev()`, blank head `top..(top+n)` ✓
  - `insert_lines` (`grid.rs:413–424`): reverse copy `(cur_row+n..=bottom).rev()`, blank `cur_row..(cur_row+n).min(bottom+1)` ✓
  - `delete_lines` (`grid.rs:444–455`): forward copy `cur_row..=(bottom-n)`, blank tail `(bottom+1-n)..=bottom` ✓
  Two new tests present:
  - `insert_lines_with_region_shifts_correctly`: 5-row grid, region 1–3, cursor row 1, insert 1 line → asserts B→row2, C→row3, blank→row1, A/E unchanged ✓
  - `delete_lines_with_region_shifts_correctly`: 5-row grid, region 1–3, cursor row 1, delete 1 line → asserts C→row1, D→row2, blank→row3, A/E unchanged ✓
- **Notes:** See Critical finding in Stage 2 — the forward-copy pattern introduces a usize underflow regression not present in the old Vec code.

---

## Stage 2: Code Quality

### Critical

- **[ISSUE-NEW-1] usize underflow panic in `scroll_up` and `delete_lines` when `n == region_height` and `top`/`cur_row` == 0**
  - **File:** `arcterm-core/src/grid.rs:182` (`scroll_up`) and `grid.rs:445` (`delete_lines`)
  - **Description:** Both forward-copy loops compute `bottom - n` as a `usize` upper bound. When `n` is clamped to `region_height = bottom + 1 - top`, this subtraction evaluates to `top - 1`. When `top == 0` (the default VT100 scroll region), `0usize - 1` overflows: **panics in debug mode; produces `usize::MAX` in release, which immediately causes an out-of-bounds index panic** on `self.cells[row]`. This is a regression — the removed `Vec::remove`/`Vec::insert` loop handled all `n` values correctly.

    The `scroll_down` and `insert_lines` methods use `(top + n..=bottom).rev()` which naturally yields an empty range when `n >= region_height` — they are unaffected. The asymmetry between methods is the root cause.

    Triggering conditions are common in practice:
    - Default scroll region is `(0, rows-1)`, so `top == 0` is normal.
    - `CSI n M` (delete lines) with `n >= rows` is a well-formed terminal sequence sent by ncurses, vim, and tmux clear/redraw paths.

  - **Remediation (scroll_up, grid.rs:182):** Replace the first loop with a bounds-checked form:
    ```rust
    if let Some(last_copy_row) = bottom.checked_sub(n) {
        for row in top..=last_copy_row {
            for col in 0..cols {
                self.cells[row][col] = self.cells[row + n][col].clone();
            }
        }
    }
    // blanking loop is correct as-is: (bottom + 1 - n)..=bottom
    ```
  - **Remediation (delete_lines, grid.rs:445):** Same pattern:
    ```rust
    if let Some(last_copy_row) = bottom.checked_sub(n) {
        for row in cur_row..=last_copy_row {
            for col in 0..cols {
                self.cells[row][col] = self.cells[row + n][col].clone();
            }
        }
    }
    ```
  - **Missing test:** Add a test `scroll_up_full_region_with_top_at_zero` that calls `set_scroll_region(0, rows-1)` then `scroll_up(rows)` and asserts all cells are blank. Same for `delete_lines_full_region_with_cursor_at_zero`.

### Important

*(none)*

### Suggestions

- **[S-1] `set_scroll_region` doc comment is slightly incomplete — `grid.rs:245–246`**
  - The doc comment says "Silently rejects invalid bounds: top >= rows, bottom >= rows, or top >= bottom." The condition `top >= bottom` also rejects `top == bottom` (a 1-row region), which is a valid DEC VT103 sequence (`CSI r` with equal values). Worth noting explicitly whether this is intentional or an off-by-one in the guard.
  - Remediation: Either document "or top >= bottom (regions must be at least 2 rows)" or change the guard to `top > bottom` if single-row regions are intended to be valid.

- **[S-2] `resize_also_resizes_alt_grid` test could assert the alt screen's own dimensions**
  - The test verifies the main grid dimensions after `leave_alt_screen()` but does not verify that `alt_grid` (the alt screen) was itself resized during the `resize()` call. A stronger test would write a cell to the alt screen at `(new_size.rows - 1, new_size.cols - 1)` before leaving, verifying that access is valid post-resize.
  - Remediation: Add a `g.cell_mut(7, 19).set_char('X')` call (within alt screen) between `resize()` and `leave_alt_screen()`.

---

## Summary

**Verdict:** REQUEST CHANGES

All four issues (007–010) are correctly implemented per spec and all planned tests are present. However, the ISSUE-010 in-place copy refactor introduces a **critical usize underflow regression** in `scroll_up` and `delete_lines` that panics on the most common scroll region configuration (top=0) when `n >= region_height`. This is a straightforward one-line fix per method using `checked_sub`. The fix must be accompanied by regression tests covering the `n == region_height, top == 0` edge case.

**Critical:** 1 | **Important:** 0 | **Suggestions:** 2
