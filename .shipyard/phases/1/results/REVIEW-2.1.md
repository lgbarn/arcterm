---
plan: "2.1"
reviewer: claude-sonnet-4-6
date: 2026-03-15
verdict: MINOR_ISSUES
---

# REVIEW-2.1 — VT Parser and Terminal Grid State Machine

## Stage 1: Spec Compliance

**Verdict:** PASS

### Task 1: Handler trait and Grid extensions (TDD)

- Status: PASS
- Evidence:
  - `arcterm-vt/src/handler.rs` defines the `Handler` trait with exactly 18 methods (put_char, newline, carriage_return, backspace, tab, bell, set_cursor_pos, cursor_up, cursor_down, cursor_forward, cursor_backward, erase_in_display, erase_in_line, set_sgr, scroll_up, scroll_down, line_feed, set_title), all with default no-op implementations.
  - `impl Handler for Grid` is in `arcterm-vt/src/handler.rs` — the coherence issue noted in the plan is correctly resolved (trait crate = impl crate).
  - `arcterm-core/src/grid.rs` adds all specified Grid extensions: `scroll_up`, `scroll_down`, `put_char_at_cursor`, `cursor()`, `set_cursor()`, `set_attrs()`, `title()`, `apply_sgr()`, plus the `current_attrs: CellAttrs` and `title: Option<String>` fields.
  - `Grid` bears `#[derive(Debug, Clone, PartialEq)]` at line 26.
  - All nine spec-required tests are present in the `handler_tests` module of `arcterm-vt/src/lib.rs` and pass.
  - SGR covers: reset (0), bold (1), italic (3), underline (4), reverse (7), fg 30-37, fg default (39), bg 40-47, bg default (49), bright fg 90-97, bright bg 100-107, 256-color 38;5;N / 48;5;N, RGB 38;2;R;G;B / 48;2;R;G;B — all verified by named test cases.
- Notes: Keeping `apply_sgr` as an inherent method on `Grid` in `arcterm-core` rather than in the VT layer is a well-reasoned architectural choice that keeps the core crate usable standalone.

### Task 2: Processor (vte bridge) (TDD)

- Status: PASS
- Evidence:
  - `arcterm-vt/src/processor.rs` defines `Processor { parser: vte::Parser }` matching the specified struct exactly.
  - `Processor::new()` and `Processor::advance<H: Handler>(&mut self, handler: &mut H, bytes: &[u8])` match the spec signatures.
  - Internal `Performer<'a, H: Handler>` holds `&'a mut H` and implements `vte::Perform`.
  - `print` -> `put_char`, `execute` dispatches 0x07/0x08/0x09/0x0A/0x0D correctly.
  - `csi_dispatch` handles A/B/C/D/H/f/J/K/m/S/T with correct 1-based-to-0-based conversion on CUP (processor.rs lines 99-113).
  - `osc_dispatch` matches params[0] == b"0" | b"2" and calls `set_title`.
  - `hook`, `put`, `unhook`, `esc_dispatch` are all no-ops as specified.
  - `lib.rs` re-exports both `Processor` and `Handler` (lines 6-7).
  - All eight specified processor tests pass.
- Notes: SGR param flattening (processor.rs lines 133-138) correctly unifies semicolon and colon sub-param encodings. The empty-params-to-`[0]` normalization (lines 134-135) is a correct guard for bare `ESC[m`.

### Task 3: Edge case tests (TDD)

- Status: PASS
- Evidence: `arcterm-vt/src/lib.rs` `mod edge_case_tests` contains 11 tests covering all nine specified scenarios (two backspace variants split into separate tests). Test count for `arcterm-vt` is 52, well above the minimum of 15.
  - Line wrapping: `line_wrapping_81_chars_in_80_col_grid`
  - Scrolling after 24 rows: `scrolling_after_24_rows_fills_content_correctly`
  - Tab stops: `tab_stop_places_char_at_col_8`, `tab_from_col_4_places_char_at_col_8`
  - 256-color SGR: `sgr_256_color_fg_via_processor`
  - RGB color SGR: `sgr_rgb_color_fg_via_processor`
  - Multi-param SGR: `sgr_multi_param_bold_fg_bg`
  - CUP defaults: `cup_no_params_positions_cursor_at_home`
  - Erase below cursor: `erase_below_cursor_at_row_10`
  - Backspace: `backspace_moves_cursor_left_via_processor`, `backspace_at_col_zero_does_not_go_negative`
- Notes: All 92 tests (40 arcterm-core, 52 arcterm-vt) pass against the live codebase, confirmed by running `cargo test --package arcterm-core --package arcterm-vt`.

---

## Stage 2: Code Quality

### Critical

None.

### Important

**1. `cursor_down` and `cursor_forward` produce unclamped intermediate values — asymmetry with `cursor_up`/`cursor_backward`**

`/Users/lgbarn/Personal/myterm/arcterm-vt/src/handler.rs`, lines 130-152

`cursor_down` passes `cur.row.saturating_add(n)` to `set_cursor`, and `cursor_forward` passes `cur.col.saturating_add(n)`. `saturating_add` on `usize` only prevents wrapping at `usize::MAX` — it does not clamp to grid bounds. Bounds clamping happens inside `set_cursor`, so the behavior is correct at runtime. The problem is asymmetry: `cursor_up` and `cursor_backward` use `saturating_sub`, which intrinsically clamps at 0, so a reader of those methods sees the full safety story in one place. A reader of `cursor_down` must also know about `set_cursor`'s clamping to be confident the method is safe. This asymmetry is a maintenance hazard: if `cursor_down` or `cursor_forward` is ever refactored to bypass `set_cursor`, the clamping silently disappears.

Remediation: Mirror the `cursor_up`/`cursor_backward` pattern by clamping explicitly before calling `set_cursor`:
```rust
fn cursor_down(&mut self, n: usize) {
    let cur = self.cursor();
    let max_row = self.size.rows.saturating_sub(1);
    self.set_cursor(CursorPos {
        row: cur.row.saturating_add(n).min(max_row),
        col: cur.col,
    });
}
```
Apply the same fix to `cursor_forward` using `self.size.cols.saturating_sub(1)`.

**2. Unchecked `cur.row + 1` in `erase_in_display` mode 0 is fragile on zero-row grids**

`/Users/lgbarn/Personal/myterm/arcterm-vt/src/handler.rs`, line 163

```rust
for r in (cur.row + 1)..rows {
```

On any grid with at least one row, `cur.row` is clamped to `[0, rows-1]` by `set_cursor`, so `cur.row + 1 <= rows` and the range is either valid or empty — no panic occurs in practice. However, `GridSize::new` has no guard against `rows == 0`, and on a 0-row grid `cur.row` is 0 (from `CursorPos::default()`), making `cur.row + 1 == 1` while `rows == 0`. The range `1..0` is empty so the loop body does not execute, but line 161 (`self.cells[cur.row][c]`) would panic before reaching it. The addition itself also becomes a potential panic point in debug builds if `rows == usize::MAX`.

Remediation: Replace `cur.row + 1` with `cur.row.saturating_add(1)` on line 163. Additionally, add a debug assertion to `GridSize::new` — `debug_assert!(rows > 0 && cols > 0, "grid dimensions must be non-zero")` — to prevent zero-dimension grids from entering the system at all.

**3. All Grid fields are `pub`, allowing invariant-breaking direct writes**

`/Users/lgbarn/Personal/myterm/arcterm-core/src/grid.rs`, lines 28-35

All six fields (`cells`, `size`, `cursor`, `dirty`, `current_attrs`, `title`) are `pub`. The `cursor` field is the most critical: `Grid` provides `cursor()` (read) and `set_cursor()` (bounds-clamped write), but the raw `pub cursor` field allows unclamped writes that silently bypass the invariant. This pattern is already embedded in tests — `grid::tests::grid_clear_resets_all_cells` at line 372 writes `g.cursor = CursorPos { row: 2, col: 2 }` directly — teaching callers that direct field access is acceptable. For `cells`, any caller can call `grid.cells.push(...)` or `grid.cells.truncate(0)`, making `cells.len() != size.rows` and corrupting all subsequent row-indexed operations.

Remediation: Make `cursor`, `current_attrs`, and `title` private (or `pub(crate)`) and route all access through the existing accessor/mutator methods. Make `cells` private and expose rendering access exclusively through the existing `rows() -> &[Vec<Cell>]`. Fix the tests that currently use direct field assignment to use `set_cursor` instead. This is the right time to enforce these invariants — no crates outside this workspace depend on the field layout yet.

### Suggestions

**4. Missing tests for `cursor_down` and `cursor_forward` boundary clamping**

`/Users/lgbarn/Personal/myterm/arcterm-vt/src/lib.rs`

`cursor_up` clamping is tested (`esc_csi_a_at_row_0_clamps_to_row_0`), but no corresponding tests exist for `cursor_down` at the last row or `cursor_forward` at the last column. Since Important finding 1 identifies these as taking a different code path from `cursor_up`/`cursor_backward`, tests are the natural complement.

Remediation: Add to `processor_tests`:
- `esc_csi_b_at_last_row_clamps` — position cursor at row 23 in a 24-row grid, feed `\x1b[5B`, assert cursor row stays at 23.
- `esc_csi_c_at_last_col_clamps` — position cursor at col 79 in an 80-col grid, feed `\x1b[5C`, assert cursor col stays at 79.

**5. `erase_in_display` mode 1 (erase above cursor) is not tested**

`/Users/lgbarn/Personal/myterm/arcterm-vt/src/lib.rs`

Mode 0 (erase below) and mode 2 (erase all) are tested. Mode 1 (erase from top to cursor, inclusive) is implemented but has no test. Some programs (`vim` `:redraw!`, `clear -x`) emit this sequence.

Remediation: Add a test that fills all cells with 'X', positions cursor at (10, 5), feeds `\x1b[1J`, and asserts rows 0-9 are cleared, cell (10, 5) is cleared, and rows 11-23 retain 'X'.

**6. `erase_in_line` mode 1 (erase left) is not tested**

`/Users/lgbarn/Personal/myterm/arcterm-vt/src/lib.rs`

Mode 0 (erase right) and mode 2 (erase entire line) are tested. Mode 1 (erase from line start to cursor, inclusive) is implemented in `handler.rs` lines 203-206 but is untested.

Remediation: Add a test that fills a row with 'X', positions cursor at col 3, feeds `\x1b[1K`, and verifies cols 0-3 are cleared and cols 4+ retain 'X'.

**7. Tab clamping at the last tab-stop interval is not tested**

`/Users/lgbarn/Personal/myterm/arcterm-vt/src/handler.rs`, lines 106-111

When the cursor is at col 72, `next_stop = (72/8 + 1) * 8 = 80`, which exceeds `max_col` (79 in an 80-col grid), so the cursor lands at 79. This is correct behavior but the clamping is implicit and there is no test documenting it. A future refactor could inadvertently wrap the cursor to column 0.

Remediation: Add a test: cursor at col 72 in an 80-col grid, feed `\t`, assert cursor is at col 79. This makes the intentional clamping explicit and regression-proof.

**8. `Processor` uses a manual `impl Default` that is a thin wrapper over `new()`**

`/Users/lgbarn/Personal/myterm/arcterm-vt/src/processor.rs`, lines 25-29

The manual `impl Default for Processor` is four lines of boilerplate that simply calls `Self::new()`. If `vte::Parser` implements `Default`, `#[derive(Default)]` on `Processor` would make this automatic and eliminate the duplication.

Remediation: Check whether `vte::Parser: Default`. If yes, replace the manual impl with `#[derive(Default)]` on the `Processor` struct. If not, add a comment on the manual impl: `// vte::Parser does not implement Default, so we cannot derive it`.

---

## Summary

**Verdict:** MINOR_ISSUES

The implementation is technically complete and correct against every requirement in the plan. All 92 tests pass, the Handler trait exactly matches the 18-method spec, SGR covers all specified color modes including 256-color and RGB, the Processor correctly bridges vte to Handler via the internal Performer pattern, and all Task 3 edge cases pass on first run. The issues found are quality concerns rather than correctness bugs: `cursor_down` and `cursor_forward` rely implicitly on `set_cursor` for bounds clamping (asymmetric with `cursor_up`/`cursor_backward`), unchecked arithmetic in `erase_in_display` is fragile on zero-row grids, and the fully-public Grid struct allows callers to bypass bounds invariants. None of these are blockers for the next wave.

Critical: 0 | Important: 3 | Suggestions: 5
