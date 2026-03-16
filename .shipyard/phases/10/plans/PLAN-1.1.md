---
phase: phase-10
plan: "1.1"
wave: 1
dependencies: []
must_haves:
  - Fix all 8 scroll_offset compile errors in arcterm-app
  - arcterm-app compiles cleanly (cargo check passes)
  - cargo clippy -p arcterm-app -- -D warnings clean
files_touched:
  - arcterm-app/src/terminal.rs
  - arcterm-app/src/main.rs
tdd: false
---

# PLAN-1.1 — scroll_offset API Migration

**Why this is Wave 1:** `arcterm-app` does not compile. Phase 9 made `Grid::scroll_offset` private and added `scroll_offset()` / `set_scroll_offset()` accessors, but `arcterm-app` still accesses the field directly in 8 locations. No testing, no ISSUE-006 fix, and no clippy check can run until compilation is restored.

**Scope:** 8 field-access replacements across 2 files. No logic changes — every replacement is a mechanical substitution from direct field access to the equivalent accessor method. The `Grid::set_scroll_offset()` setter already performs the same `offset.min(scrollback.len())` clamping that callers were doing manually, so callers can drop their own clamping arithmetic.

---

## Tasks

<task id="1" files="arcterm-app/src/terminal.rs" tdd="false">
  <action>In `Terminal::set_scroll_offset` (line 197-200), replace the method body. Remove the manual `scrollback.len()` read and direct field assignment. Delegate to `self.grid_state.grid.set_scroll_offset(offset)` which performs identical clamping internally. Remove the `#[allow(dead_code)]` attribute on line 196 — this method is used by callers holding a `Terminal` reference.</action>
  <verify>cargo check -p arcterm-app 2>&1 | grep -c "error" || echo "0 errors"</verify>
  <done>The `terminal.rs:199` compile error (E0616: field `scroll_offset` is private) is gone. The method body is a single delegation call with no manual field access.</done>
</task>

<task id="2" files="arcterm-app/src/main.rs" tdd="false">
  <action>Fix the remaining 7 compile errors in `main.rs` by replacing direct `grid.scroll_offset` field access with accessor calls. All changes are mechanical substitutions:

1. **Line ~1692 (PTY got_data read):** Change `grid.scroll_offset > 0` to `grid.scroll_offset() > 0`.
2. **Line ~1694 (PTY got_data write):** Change `grid.scroll_offset = 0` to `grid.set_scroll_offset(0)`.
3. **Line ~1981 (MouseWheel read):** Change `grid.scroll_offset as i32` to `grid.scroll_offset() as i32`.
4. **Line ~1984 (MouseWheel write):** Change `grid.scroll_offset = new_offset` to `grid.set_scroll_offset(new_offset)`. The explicit `max_offset` / `clamp` arithmetic can be simplified to `.max(0) as usize` since the setter handles the upper bound clamp internally.
5. **Line ~2343 (search overlay read):** Change `grid.scroll_offset` to `grid.scroll_offset()`.
6. **Line ~2651 (SearchAction::NextMatch write):** Change `terminal.grid_mut().scroll_offset = ...` to `terminal.grid_mut().set_scroll_offset(...)`.
7. **Line ~2671 (SearchAction::PrevMatch write):** Same pattern as NextMatch.

Note: Lines referencing `review.scroll_offset` (overlay::OverlayReviewState) are a different struct and must NOT be changed.</action>
  <verify>cargo check -p arcterm-app 2>&1 | grep "error" | grep -v "warning" | wc -l | tr -d ' '</verify>
  <done>Output is `0`. `cargo check -p arcterm-app` succeeds with zero errors. All 8 original E0616 errors are resolved.</done>
</task>

<task id="3" files="arcterm-app/src/terminal.rs, arcterm-app/src/main.rs" tdd="false">
  <action>Run clippy on `arcterm-app` and fix any warnings introduced by the migration. Expected: no new warnings since the changes are strictly narrower (removing manual clamping, replacing field access with method calls). Confirm the full crate compiles and lints clean.</action>
  <verify>cargo clippy -p arcterm-app -- -D warnings 2>&1 | tail -5</verify>
  <done>`cargo clippy -p arcterm-app -- -D warnings` exits with status 0. No warnings, no errors. The crate is ready for Wave 2 work.</done>
</task>
