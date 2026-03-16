---
phase: foundation-fixes
plan: "1.1"
wave: 1
dependencies: []
must_haves:
  - ISSUE-007 set_scroll_region bounds validation
  - ISSUE-008 resize alt_grid when present
  - ISSUE-009 scroll_offset encapsulation with validated setter
  - ISSUE-010 in-place copy scroll operations replacing O(n*rows) remove/insert
files_touched:
  - arcterm-core/src/grid.rs
tdd: true
---

# PLAN-1.1 — Grid Fixes (arcterm-core)

## Context

Four bugs in `arcterm-core/src/grid.rs` affect terminal correctness and safety:

1. **ISSUE-007**: `set_scroll_region()` stores any `(top, bottom)` pair without validation. If `bottom >= rows`, `scroll_up()` panics on `cells.remove(bottom)`. If `top >= bottom`, scrolls are silent no-ops.

2. **ISSUE-008**: `resize()` updates `self.cells` and `self.size` but ignores `self.alt_grid`. When the terminal resizes while in alt-screen mode, `leave_alt_screen()` restores pre-resize dimensions, causing panics on subsequent cell access.

3. **ISSUE-009**: `scroll_offset` is a public field with no write validation. Callers can set it beyond `scrollback.len()` with no feedback. Encapsulating it with a clamping setter makes the API honest. **Note:** making the field private will break `arcterm-app/src/main.rs` which accesses it directly. This is expected and handled in Phase 10.

4. **ISSUE-010**: `scroll_up()`, `scroll_down()`, `insert_lines()`, and `delete_lines()` use `Vec::remove`/`Vec::insert` loops — O(n * rows) per scroll. Replace with in-place index-based copy (O(rows * cols) total) matching the pattern already used in `arcterm-vt/src/handler.rs`.

All four touch only `arcterm-core/src/grid.rs`. No other crate or file is modified.

## Dependencies

None. This plan has no dependencies on other Phase 9 plans.

## Tasks

<task id="1" files="arcterm-core/src/grid.rs" tdd="true">
  <action>
  Fix ISSUE-007 and ISSUE-008 with tests.

  **ISSUE-007** — Add bounds validation at the top of `set_scroll_region()` (line 232):
  ```rust
  if top >= self.size.rows || bottom >= self.size.rows || top >= bottom {
      return;
  }
  ```
  Add three tests below the existing scroll-region test group (after line 883):
  - `set_scroll_region_rejects_inverted_bounds`: call with top=5, bottom=2 on a 10-row grid, assert `scroll_region` is `None`
  - `set_scroll_region_rejects_bottom_out_of_range`: call with bottom=grid.size.rows, assert `scroll_region` is `None`
  - `set_scroll_region_rejects_top_out_of_range`: call with top=grid.size.rows, assert `scroll_region` is `None`

  **ISSUE-008** — Append to the end of `resize()` (after line 532, before closing brace):
  ```rust
  if let Some(ref mut ag) = self.alt_grid {
      ag.resize(new_size);
  }
  ```
  Add one test:
  - `resize_also_resizes_alt_grid`: create grid, call `enter_alt_screen()`, call `resize(new_size)`, call `leave_alt_screen()`, assert `cells.len() == new_size.rows` and `cells[0].len() == new_size.cols`
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-core -- set_scroll_region_rejects && cargo test -p arcterm-core -- resize_also_resizes_alt_grid</verify>
  <done>All four new tests pass. `set_scroll_region` silently rejects invalid bounds. `resize` propagates to `alt_grid`.</done>
</task>

<task id="2" files="arcterm-core/src/grid.rs" tdd="true">
  <action>
  Fix ISSUE-009 — encapsulate `scroll_offset`.

  1. Change `pub scroll_offset: usize` (line 92) to `scroll_offset: usize` (remove `pub`).

  2. Add two methods to the `impl Grid` block:
  ```rust
  /// Set the scroll offset, clamping to the current scrollback length.
  pub fn set_scroll_offset(&mut self, offset: usize) {
      self.scroll_offset = offset.min(self.scrollback.len());
  }

  /// Current scroll offset (0 = live view).
  pub fn scroll_offset(&self) -> usize {
      self.scroll_offset
  }
  ```

  3. Update the existing test at line 1042 that sets `g.scroll_offset = 1` directly — change to `g.set_scroll_offset(1)`. Also update any read of `g.scroll_offset` in tests to use `g.scroll_offset()`.

  4. Add two new tests:
  - `set_scroll_offset_clamps_to_scrollback_len`: push 5 lines to scrollback, call `set_scroll_offset(100)`, assert `scroll_offset() == 5`
  - `set_scroll_offset_zero_is_valid`: call `set_scroll_offset(0)`, assert `scroll_offset() == 0`

  **Cross-crate breakage note:** `arcterm-app/src/main.rs` accesses `scroll_offset` directly. After this change, `arcterm-app` will not compile until it is updated to use the new accessor methods. This is expected — Phase 10 handles `arcterm-app` changes. Verify only with `cargo test -p arcterm-core`.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-core -- scroll_offset</verify>
  <done>Field is private. `set_scroll_offset` clamps correctly. Both new tests and the updated existing test pass. `cargo test -p arcterm-core` passes.</done>
</task>

<task id="3" files="arcterm-core/src/grid.rs" tdd="true">
  <action>
  Fix ISSUE-010 — replace O(n*rows) scroll loops with in-place copy.

  Replace four code sections with in-place index-based copy patterns:

  **1. `scroll_up` partial-region path (lines 181-184)** — replace the `for _ in 0..n { cells.remove(top); cells.insert(bottom, blank_row()); }` loop with:
  ```rust
  let cols = self.size.cols;
  for row in top..=(bottom - n) {
      for col in 0..cols {
          self.cells[row][col] = self.cells[row + n][col].clone();
      }
  }
  for row in (bottom + 1 - n)..=bottom {
      for col in 0..cols {
          self.cells[row][col] = Cell::default();
      }
  }
  ```

  **2. `scroll_down` partial-region path (lines 215-219)** — replace with:
  ```rust
  let cols = self.size.cols;
  for row in (top + n..=bottom).rev() {
      for col in 0..cols {
          self.cells[row][col] = self.cells[row - n][col].clone();
      }
  }
  for row in top..(top + n) {
      for col in 0..cols {
          self.cells[row][col] = Cell::default();
      }
  }
  ```

  **3. `insert_lines` (lines 386-391)** — replace with:
  ```rust
  let cols = self.size.cols;
  for row in (cur_row + n..=bottom).rev() {
      for col in 0..cols {
          self.cells[row][col] = self.cells[row - n][col].clone();
      }
  }
  for row in cur_row..(cur_row + n).min(bottom + 1) {
      for col in 0..cols {
          self.cells[row][col] = Cell::default();
      }
  }
  ```

  **4. `delete_lines` (lines 411-415)** — replace with:
  ```rust
  let cols = self.size.cols;
  for row in cur_row..=(bottom - n) {
      for col in 0..cols {
          self.cells[row][col] = self.cells[row + n][col].clone();
      }
  }
  for row in (bottom + 1 - n)..=bottom {
      for col in 0..cols {
          self.cells[row][col] = Cell::default();
      }
  }
  ```

  Existing tests `scroll_up_with_region_only_affects_region_rows` and `scroll_down_with_region_only_affects_region_rows` provide regression coverage. Add two more tests:
  - `insert_lines_with_region_shifts_correctly`: set a scroll region, write content, call `insert_lines`, verify cell content positions
  - `delete_lines_with_region_shifts_correctly`: same pattern for `delete_lines`
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-core -- scroll && cargo test -p arcterm-core -- insert_lines && cargo test -p arcterm-core -- delete_lines</verify>
  <done>All scroll, insert_lines, and delete_lines tests pass. No `Vec::remove`/`Vec::insert` calls remain in the partial-region scroll paths. `cargo test -p arcterm-core` passes fully.</done>
</task>

## Final Verification

```bash
cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-core && cargo clippy -p arcterm-core -- -D warnings
```

All `arcterm-core` tests pass. Clippy is clean with `-D warnings`.
