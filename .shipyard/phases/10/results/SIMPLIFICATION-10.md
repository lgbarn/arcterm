# Simplification Report
**Phase:** 10 (Wave 2 — API migration + ctrl_char_byte helper + cursor block glyph)
**Date:** 2026-03-16
**Files analyzed:** 4 (input.rs, main.rs, terminal.rs, text.rs)
**Findings:** 2 low-priority

---

## High Priority

None.

## Medium Priority

None.

## Low Priority

### 1. Duplicated `cursor_col` expression in `prepare_grid` and `prepare_grid_at`

- **Type:** Refactor
- **Locations:** `arcterm-render/src/text.rs:189`, `arcterm-render/src/text.rs:280`
- **Description:** The expression `if row_idx == cursor.row { Some(cursor.col) } else { None }` appears verbatim in both `prepare_grid` (single-pane path) and `prepare_grid_at` (multi-pane path). This is 2 occurrences — below the Rule of Three threshold — so extraction is not required now. It is worth noting for when a third call site is added.
- **Suggestion:** When a third call site appears, extract to an inline helper or a method on `CursorPos` such as `fn col_if_row(&self, row_idx: usize) -> Option<usize>`. No action needed today.

### 2. Dead `is_cursor_row` binding in `prepare_grid_at`

- **Type:** Remove
- **Locations:** `arcterm-render/src/text.rs:266-271`
- **Description:** `let is_cursor_row = row_idx == cursor.row;` is computed then immediately suppressed with `let _ = is_cursor_row;`. The comment explains this is intentional — the hash-skip optimisation is not yet implemented for multi-pane. However, the variable provides no value in its current form. The comment alone is sufficient; the unused binding adds visual noise.
- **Suggestion:** Remove both `let is_cursor_row = ...;` and `let _ = is_cursor_row;`. Keep the comment about the planned hash cache optimisation. This is a one-line cleanup.
- **Impact:** 2 lines removed, no behavior change.

---

## Summary

- **Duplication found:** 1 near-duplicate (2 occurrences, below extraction threshold)
- **Dead code found:** 1 suppressed binding (`is_cursor_row` in `prepare_grid_at`)
- **Complexity hotspots:** 0 functions exceeding thresholds
- **AI bloat patterns:** 0 instances

The two new helpers (`ctrl_char_byte`, `substitute_cursor_char`) are cleanly extracted, correctly scoped `pub(crate)`, and each has a single well-defined responsibility. The `main.rs` API migration to accessor methods is mechanical and uniform. Tests are tight and directly target the extracted pure functions. The `terminal.rs` delegation wrapper (`set_scroll_offset`) is annotated `#[allow(dead_code)]` with a documented future use — this is intentional and should not be removed.

## Recommendation

No simplification is required before shipping. The only actionable item is the 2-line dead binding removal in `prepare_grid_at` (`arcterm-render/src/text.rs:266-271`), which is cosmetic. The near-duplicate `cursor_col` expression is below threshold and should be revisited only when a third call site appears. Phase 10 code is clean.
