# REVIEW-2.1 — Regression Tests + ISSUE-006 Cursor Fix

**Plan:** PLAN-2.1
**Phase:** 10, Wave 2
**Reviewed:** 2026-03-16
**Reviewer:** review-agent

---

## Stage 1: Spec Compliance

**Verdict:** PASS

### Task 1: ctrl_char_byte helper + ISSUE-003 regression tests

- Status: PASS
- Evidence: `arcterm-app/src/input.rs` — `pub(crate) fn ctrl_char_byte(ch: char) -> Option<Vec<u8>>` is extracted above `translate_key_event`. The old inline logic (lines 22–41 pre-diff) is fully removed from `translate_key_event`, which now calls `ctrl_char_byte(ch)` in the `Key::Character(s) if ctrl` arm. Three tests are present in `mod tests` and all pass live:
  - `ctrl_backslash_sends_0x1c` — `ctrl_char_byte('\\')` returns `Some(vec![0x1c])`. PASS.
  - `ctrl_bracket_right_sends_0x1d` — `ctrl_char_byte(']')` returns `Some(vec![0x1d])`. PASS.
  - `ctrl_a_sends_0x01` — `ctrl_char_byte('a')` returns `Some(vec![0x01])`. PASS.
- Notes: The helper correctly handles all four documented mappings (alphabetic, `[`, `\`, `]`). `translate_key_event` delegates fully — no mapping logic is duplicated. The import in the test module was updated to `use super::{ctrl_char_byte, translate_named_key}`. `cargo test -p arcterm-app -- ctrl_` reports 3/9 matching tests passing; full suite shows 301 passed, 0 failed.

### Task 2: ISSUE-006 cursor block glyph implementation

- Status: PASS
- Evidence: `arcterm-render/src/text.rs` — `pub(crate) fn substitute_cursor_char(row: &[arcterm_core::Cell], cursor_col: Option<usize>) -> Vec<char>` is added at line 655. The condition `cursor_col == Some(i) && (cell.c == ' ' || cell.c == '\0')` substitutes `'\u{2588}'` for blank/null cells at the cursor column; all other cells return `cell.c` unchanged.
  - `shape_row_into_buffer` signature updated to include `cursor_col: Option<usize>` as a fifth parameter.
  - `prepare_grid` call site (line 189): `let cursor_col = if row_idx == cursor.row { Some(cursor.col) } else { None };` — correct pattern.
  - `prepare_grid_at` call site (line 280): identical pattern — correct.
  - Cell data is never modified; substitution is isolated to `substitute_cursor_char`, which is called only from `shape_row_into_buffer`. Stored `Cell` structs are untouched.
- Notes: The builder chose the plan's "option B" — extract a pure testable helper — because `FontSystem`/`Buffer` require font discovery at test time. This is a sound deviation noted transparently in SUMMARY-2.1. The helper name `substitute_cursor_char` (vs. plan's `substitute_cursor_blank`) is more accurate and is not a meaningful deviation. The `hash_row` function was confirmed to already incorporate `cursor.col`, so cursor movement on/off a blank cell correctly invalidates the row hash without code changes.

### Task 3: cursor_on_blank_substitutes_block_glyph test + full suite

- Status: PASS
- Evidence: `arcterm-render/src/text.rs` `mod tests` block contains:
  - `cursor_on_blank_substitutes_block_glyph` — 5-cell default row, `Some(2)` cursor col; asserts `chars[2] == '\u{2588}'` and all others remain `' '`. Exactly matches the plan's spec.
  - `no_cursor_no_substitution` — `cursor_col: None`; all cells remain `' '`. Boundary case beyond plan minimum.
  - `cursor_on_non_blank_no_substitution` — `'A'` at col 2 with `cursor_col: Some(2)`; asserts no substitution. Boundary case beyond plan minimum.
  - All existing `hash_row_*` tests remain present and passing.
- Notes: `cargo test -p arcterm-render` shows 41 passed, 0 failed (up from 36 per SUMMARY). `cargo test -p arcterm-app` shows 301 passed, 0 failed. `cargo clippy -p arcterm-app -p arcterm-render -- -D warnings` exits cleanly. All done criteria are satisfied.

---

## Stage 2: Code Quality

### Critical

None.

### Important

- **No test for `cursor_col` pointing past the end of the row** in `arcterm-render/src/text.rs:655–669`
  - `substitute_cursor_char` uses `.enumerate()` over `row.iter()`, so `cursor_col == Some(i)` for an out-of-range column simply never matches. The substitution silently produces no block glyph with no error. This is safe, but if the caller supplies `cursor.col >= row.len()` (possible after a terminal resize where the cursor has not yet been clamped, or from a malformed VT sequence), the cursor will be invisible on a blank cell without any diagnostic. A test asserting the out-of-bounds case is a no-op (returns all original chars) would document the intended defensive behavior.
  - Remediation: Add test `cursor_col_out_of_bounds_no_panic` in `mod tests`: construct a 5-cell row, call `substitute_cursor_char(&row, Some(10))`, assert all characters remain `' '`. This documents the guarantee and guards against future changes that might panic on an out-of-range index.

- **`substitute_cursor_char` allocates a `Vec<char>` on every shaped row** (`arcterm-render/src/text.rs:658–669`)
  - For a 220-column × 50-row grid at 60 fps, this is 660,000 small `Vec` allocations per second. The current design is correct and clear, and is appropriate for the TDD/shipping phase. However, in a performance-sensitive rendering path it is worth noting as a future optimization target (the substitution could be done inline in the `zip`/`map` chain without materializing an intermediate vector).
  - Remediation: No change required now. When profiling surfaces this as a bottleneck, consider folding `substitute_cursor_char` logic into the `zip.map` closure in `shape_row_into_buffer` and eliminating the intermediate `Vec<char>`.

### Suggestions

- **`ctrl_char_byte` is `pub(crate)` but has no doc-test** (`arcterm-app/src/input.rs:14`)
  - The doc comment describes all four mappings clearly. Adding inline `# Examples` with `///` rustdoc examples would make the mappings machine-verified, consistent with the level of documentation present elsewhere in the codebase.
  - Remediation: Add a `# Examples` section to the `ctrl_char_byte` doc comment demonstrating `ctrl_char_byte('a')` → `Some(vec![0x01])`.

- **`Cell::default()` character is `' '` (space); the `'\0'` branch in `substitute_cursor_char` is not exercised by any test** (`arcterm-render/src/text.rs:662`)
  - The null-character guard (`cell.c == '\0'`) is defensive code for cells initialized to zero. No test constructs a `Cell` with `c == '\0'` to verify this branch. If the branch is intentionally needed (e.g., for zero-initialized cells from a future `unsafe` allocation path), it deserves a test; if cells are always initialized to `' '` via `Cell::default()`, the guard is dead code and should be removed to avoid confusion.
  - Remediation: Either add a test `cursor_on_null_cell_substitutes_block_glyph` that sets `cell.c = '\0'` and verifies U+2588 is returned, or remove the `|| cell.c == '\0'` branch with a comment explaining why `Cell::default()` guarantees `' '` makes the guard unnecessary.

---

## ISSUES.md Appendix

The following non-blocking findings are appended to `.shipyard/ISSUES.md`:

- **REVIEW-2.1-A** (Important): No test for `cursor_col` out-of-bounds in `substitute_cursor_char`. File: `arcterm-render/src/text.rs:655`.
- **REVIEW-2.1-B** (Suggestion): `'\0'` guard in `substitute_cursor_char` is untested / potentially dead code. File: `arcterm-render/src/text.rs:662`.

---

## Summary

**Verdict:** APPROVE

All three plan tasks are correctly implemented and verified against live test runs (301/301 arcterm-app, 41/41 arcterm-render, zero clippy warnings). The `ctrl_char_byte` refactor cleanly extracts and delegates the ctrl-character logic; the `substitute_cursor_char` pure function correctly isolates the ISSUE-006 render-path substitution from stored cell data; both call sites in `prepare_grid` and `prepare_grid_at` pass the cursor column correctly. The two non-blocking findings (out-of-bounds cursor column test gap, `'\0'` guard coverage) do not block merge.

Critical: 0 | Important: 1 | Suggestions: 2
