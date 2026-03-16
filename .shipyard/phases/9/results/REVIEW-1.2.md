# REVIEW-1.2 — VT Regression Tests (PLAN-1.2)

**Reviewer:** shipyard:reviewer
**Commit reviewed:** `57ff87b`
**Plan:** `.shipyard/phases/9/plans/PLAN-1.2.md`

---

## Stage 1: Spec Compliance

**Verdict:** FAIL

### Task 1: ISSUE-011 — esc_dispatch intermediates guard

- **Status:** PASS
- **Evidence:** `arcterm-vt/src/processor.rs:939` — `esc_dispatch_with_intermediates_does_not_save_cursor` exists and feeds `b"\x1b(7"`, asserting `gs.saved_cursor.is_none()`. `arcterm-vt/src/processor.rs:954` — `esc_dispatch_bare_esc7_saves_cursor` exists, saves via `ESC 7`, relocates cursor, restores via `ESC 8`, asserts `gs.grid.cursor() == CursorPos { row: 3, col: 5 }`.
- **Notes:** Builder deviated from the plan's negative-test assertion. The plan used `assert_eq!(gs.grid.cursor(), ...)` which would be a vacuous check (save_cursor does not move the cursor; it would always pass). The builder substituted `assert!(gs.saved_cursor.is_none())` — this is a **better assertion** that directly proves the guard prevented the call. Deviation is beneficial and both done criteria are satisfied.

### Task 2: ISSUE-012 — modes 47/1047 and mouse modes 1000/1006

- **Status:** FAIL
- **Evidence:**
  - `set_mode_47_enters_alt_screen` (line 971): present, asserts `gs.modes.alt_screen`.
  - `reset_mode_47_leaves_alt_screen` (line 979): present, asserts `!gs.modes.alt_screen`.
  - `set_mode_1000_enables_mouse_click_report` (line 989): present.
  - `reset_mode_1000_disables_mouse_click_report` (line 997): present.
  - `set_mode_1006_enables_sgr_mouse_ext` (line 1007): present.
  - Mode 1047 — **no tests exist.** `grep -n "1047"` returns only a comment at line 967 (section header), no `?1047h` or `?1047l` test sequences anywhere in the file.
- **Notes:** The plan's `<done>` criterion explicitly states: *"modes **47, 1047** enter/leave alt screen correctly"*. The `must_haves` frontmatter also lists `ISSUE-012 regression tests for modes 47/1047`. Two tests are required for mode 1047 (`set_mode_1047_enters_alt_screen` and `reset_mode_1047_leaves_alt_screen`); neither was written. The builder's summary does not acknowledge the omission — it only reports five tests added (all for 47 and mouse modes). The field assertion adaptation (`gs.modes.alt_screen` instead of `gs.grid.alt_grid.is_some()`) is correct and appropriate given the real struct layout.

### Task 3: ISSUE-013 — newline cursor-above-scroll-region behavior

- **Status:** PASS
- **Evidence:** `arcterm-vt/src/processor.rs:1014` — `newline_cursor_above_scroll_region_advances_into_region` exists. Uses `make_gs_with_size(10, 80)` (new helper defined at line 927). Sets scroll region via `CSI 4;8 r` (rows 3–7 zero-indexed), positions cursor at row 0 via `CSI 1;1 H`, then issues 8 individual `\n` bytes with per-step row assertions: 0→1→2→3→4→5→6→7, then one more `\n` with final assertion `row == 7` (scroll triggered, cursor stays pinned).
- **Notes:** Test faithfully implements the plan's specification with meaningful per-step assertions. The introduction of `make_gs_with_size` is correct — the plan explicitly required it.

---

## Stage 2: Code Quality

**Not performed — Stage 1 failed.**

---

## Summary

**Verdict:** REQUEST CHANGES

Tasks 1 and 3 are correctly implemented. Task 2 is incomplete: mode 1047 enter/leave alt-screen tests are absent despite being explicitly required by both the `must_haves` frontmatter and the `<done>` criterion. Two tests must be added before this plan can be considered complete.

**Required fix:** Add the following two tests to `mod phase9_regression_tests` in `arcterm-vt/src/processor.rs`:

```rust
#[test]
fn set_mode_1047_enters_alt_screen() {
    let mut gs = make_gs();
    let mut proc = Processor::new();
    proc.advance(&mut gs, b"\x1b[?1047h");
    assert!(gs.modes.alt_screen, "mode 1047 should enter alt screen");
}

#[test]
fn reset_mode_1047_leaves_alt_screen() {
    let mut gs = make_gs();
    let mut proc = Processor::new();
    proc.advance(&mut gs, b"\x1b[?1047h");
    assert!(gs.modes.alt_screen);
    proc.advance(&mut gs, b"\x1b[?1047l");
    assert!(!gs.modes.alt_screen, "mode 1047 reset should leave alt screen");
}
```

Critical: 0 | Important: 1 (missing mode 1047 tests — spec compliance gap) | Suggestions: 0
