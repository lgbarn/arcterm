# SUMMARY-1.1 — Grid Fixes (arcterm-core)

**Plan:** PLAN-1.1
**Phase:** 9
**Status:** Complete
**File modified:** `arcterm-core/src/grid.rs` only

---

## Task 1 — ISSUE-007 + ISSUE-008

**Commits:** `9abfcd4`

### ISSUE-007: set_scroll_region bounds validation
Added guard at top of `set_scroll_region()`:
```rust
if top >= self.size.rows || bottom >= self.size.rows || top >= bottom {
    return;
}
```
Prevents out-of-bounds panics when `scroll_up` tries to `cells.remove(bottom)`.

**Tests added (4):**
- `set_scroll_region_rejects_inverted_bounds` — top=5, bottom=2 → None
- `set_scroll_region_rejects_bottom_out_of_range` — bottom=rows → None
- `set_scroll_region_rejects_top_out_of_range` — top=rows → None
- `resize_also_resizes_alt_grid` — (see ISSUE-008)

### ISSUE-008: alt_grid resize propagation
Appended to end of `resize()`:
```rust
if let Some(ref mut ag) = self.alt_grid {
    ag.resize(new_size);
}
```
Prevents `leave_alt_screen()` from restoring stale pre-resize dimensions.

**Tests added (1):**
- `resize_also_resizes_alt_grid` — enter alt, resize, leave alt, assert dimensions match new_size

---

## Task 2 — ISSUE-009: scroll_offset encapsulation

**Commit:** `1996db9`

- Changed `pub scroll_offset: usize` → `scroll_offset: usize` (private)
- Added `pub fn set_scroll_offset(&mut self, offset: usize)` — clamps to `scrollback.len()`
- Added `pub fn scroll_offset(&self) -> usize` — getter
- Updated existing test `rows_for_viewport_with_scroll_offset_shows_scrollback_mix` to use `set_scroll_offset(1)`

**Note:** `arcterm-app/src/main.rs` accesses `scroll_offset` directly and will not compile until updated in Phase 10. Verified with `-p arcterm-core` only as specified.

**Tests added (2):**
- `set_scroll_offset_clamps_to_scrollback_len` — 5-row scrollback, set offset 100 → offset==5
- `set_scroll_offset_zero_is_valid` — set offset 0 → offset==0

---

## Task 3 — ISSUE-010: In-place scroll operations

**Commit:** `a243f8a`

Replaced four O(n·rows) `Vec::remove`/`Vec::insert` loops with O(rows·cols) in-place index copy:

| Method | Old pattern | New pattern |
|--------|-------------|-------------|
| `scroll_up` (partial region) | `for _ in 0..n { cells.remove(top); cells.insert(bottom, blank) }` | Index copy forward, blank tail |
| `scroll_down` (partial region) | `for _ in 0..n { cells.remove(bottom); cells.insert(top, blank) }` | Index copy backward (rev), blank head |
| `insert_lines` | Same remove/insert pattern | Index copy backward (rev), blank inserted rows |
| `delete_lines` | Same remove/insert pattern | Index copy forward, blank tail rows |

**Tests added (2):**
- `insert_lines_with_region_shifts_correctly` — region 1-3, insert at row 1, verify B→row2, C→row3, blank→row1
- `delete_lines_with_region_shifts_correctly` — region 1-3, delete at row 1, verify C→row1, D→row2, blank→row3

---

## Final Verification

```
cargo test -p arcterm-core    → 63 passed, 0 failed
cargo clippy -p arcterm-core -- -D warnings → clean (no warnings)
```

## Deviations

None. Plan executed exactly as specified. No architectural changes or out-of-scope modifications.

## Cross-crate Impact

- `arcterm-app` will fail to compile due to ISSUE-009 (`scroll_offset` is now private). This is expected per the plan; Phase 10 will update `arcterm-app` to use the new accessor API.
