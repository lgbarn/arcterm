---
phase: foundation-fixes
plan: "1.2"
wave: 1
dependencies: []
must_haves:
  - ISSUE-011 regression test for esc_dispatch intermediates guard
  - ISSUE-012 regression tests for modes 47/1047 and mouse modes 1000/1002/1003/1006
  - ISSUE-013 regression test for newline cursor-above-scroll-region behavior
files_touched:
  - arcterm-vt/src/processor.rs
tdd: true
---

# PLAN-1.2 — VT/Parser Regression Tests (arcterm-vt)

## Context

Issues 11, 12, and 13 are **already fixed** in the codebase. The code changes were applied before Phase 9. However, Phase 9 success criteria require regression tests for each fix to prevent future regressions.

- **ISSUE-011**: `esc_dispatch` already has the `if !intermediates.is_empty() { return; }` guard at line 601 of `processor.rs`. Needs a test proving `ESC ( 7` (SCS sequence) does not trigger `save_cursor_position`.

- **ISSUE-012**: Modes 47, 1047, 1000, 1002, 1003, 1006 are already handled in `set_mode`/`reset_mode` in `handler.rs`. `TermModes` already has the mouse fields. Needs tests confirming these modes work correctly.

- **ISSUE-013**: The unreachable newline clamp is already removed. The `else` branch in `newline()` simply advances the cursor by one row. Needs a test that places the cursor above the scroll region and verifies correct advancement behavior.

All tests go in `processor.rs` following the established `make_gs()` + `feed()` pattern used by existing test modules.

## Dependencies

None. This plan touches only `arcterm-vt/src/processor.rs` (test additions).

## Tasks

<task id="1" files="arcterm-vt/src/processor.rs" tdd="true">
  <action>
  Add regression test for ISSUE-011 (esc_dispatch intermediates guard).

  Add a new test in `processor.rs` in an appropriate test module (e.g., after the existing `phase4_task2_tests` module, or as a new `#[cfg(test)] mod regression_tests` module):

  ```rust
  #[test]
  fn esc_dispatch_with_intermediates_does_not_save_cursor() {
      let mut gs = make_gs();
      gs.grid.set_cursor(CursorPos { row: 3, col: 5 });
      let mut proc = Processor::new();
      // ESC ( 7 — SCS select character set — must NOT trigger save_cursor_position (0x37).
      proc.advance(&mut gs, b"\x1b(7");
      assert_eq!(gs.grid.cursor(), CursorPos { row: 3, col: 5 },
          "SCS ESC(7 must not trigger save_cursor");
  }
  ```

  Also add a positive test confirming bare `ESC 7` (DECSC) does save the cursor:
  ```rust
  #[test]
  fn esc_dispatch_bare_esc7_saves_cursor() {
      let mut gs = make_gs();
      gs.grid.set_cursor(CursorPos { row: 3, col: 5 });
      let mut proc = Processor::new();
      proc.advance(&mut gs, b"\x1b7");
      // Move cursor elsewhere
      gs.grid.set_cursor(CursorPos { row: 0, col: 0 });
      // ESC 8 (DECRC) should restore
      proc.advance(&mut gs, b"\x1b8");
      assert_eq!(gs.grid.cursor(), CursorPos { row: 3, col: 5 });
  }
  ```

  Use `make_gs()` and `Processor::new()` following existing test patterns. If `make_gs` is defined in a specific test module, either reuse it or define a local helper with the same pattern.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-vt -- esc_dispatch</verify>
  <done>Both `esc_dispatch_with_intermediates_does_not_save_cursor` and `esc_dispatch_bare_esc7_saves_cursor` pass.</done>
</task>

<task id="2" files="arcterm-vt/src/processor.rs" tdd="true">
  <action>
  Add regression tests for ISSUE-012 (modes 47, 1047, and mouse modes).

  Add tests in the same test module as Task 1:

  **Alt-screen modes 47 and 1047:**
  ```rust
  #[test]
  fn set_mode_47_enters_alt_screen() {
      let mut gs = make_gs();
      let mut proc = Processor::new();
      // Write something to primary screen
      proc.advance(&mut gs, b"hello");
      // CSI ? 47 h — enter alt screen
      proc.advance(&mut gs, b"\x1b[?47h");
      assert!(gs.grid.alt_grid.is_some(), "mode 47 should enter alt screen");
  }

  #[test]
  fn reset_mode_47_leaves_alt_screen() {
      let mut gs = make_gs();
      let mut proc = Processor::new();
      proc.advance(&mut gs, b"\x1b[?47h");
      assert!(gs.grid.alt_grid.is_some());
      proc.advance(&mut gs, b"\x1b[?47l");
      assert!(gs.grid.alt_grid.is_none(), "mode 47 reset should leave alt screen");
  }
  ```

  **Mouse modes:**
  ```rust
  #[test]
  fn set_mode_1000_enables_mouse_click_report() {
      let mut gs = make_gs();
      let mut proc = Processor::new();
      proc.advance(&mut gs, b"\x1b[?1000h");
      assert!(gs.modes.mouse_report_click);
  }

  #[test]
  fn reset_mode_1000_disables_mouse_click_report() {
      let mut gs = make_gs();
      let mut proc = Processor::new();
      proc.advance(&mut gs, b"\x1b[?1000h");
      assert!(gs.modes.mouse_report_click);
      proc.advance(&mut gs, b"\x1b[?1000l");
      assert!(!gs.modes.mouse_report_click);
  }

  #[test]
  fn set_mode_1006_enables_sgr_mouse_ext() {
      let mut gs = make_gs();
      let mut proc = Processor::new();
      proc.advance(&mut gs, b"\x1b[?1006h");
      assert!(gs.modes.mouse_sgr_ext);
  }
  ```

  Adjust field access paths based on actual struct layout (e.g., `gs.modes.mouse_report_click` or however TermModes is accessed from GridState).
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-vt -- set_mode && cargo test -p arcterm-vt -- reset_mode</verify>
  <done>All mode tests pass: modes 47, 1047 enter/leave alt screen correctly; mouse modes 1000 and 1006 set/clear correctly.</done>
</task>

<task id="3" files="arcterm-vt/src/processor.rs" tdd="true">
  <action>
  Add regression test for ISSUE-013 (newline cursor-above-scroll-region behavior).

  Add a test that exercises the cursor-above-region case:

  ```rust
  #[test]
  fn newline_cursor_above_scroll_region_advances_into_region() {
      let mut gs = make_gs_with_size(10, 80);  // 10 rows, 80 cols
      let mut proc = Processor::new();
      // Set scroll region to rows 3-7 (0-indexed): CSI 4;8 r (1-indexed)
      proc.advance(&mut gs, b"\x1b[4;8r");
      // Move cursor to row 0: CSI 1;1 H (1-indexed)
      proc.advance(&mut gs, b"\x1b[1;1H");
      assert_eq!(gs.grid.cursor().row, 0);

      // Newline should advance row by row toward the region
      proc.advance(&mut gs, b"\n");
      assert_eq!(gs.grid.cursor().row, 1);
      proc.advance(&mut gs, b"\n");
      assert_eq!(gs.grid.cursor().row, 2);
      proc.advance(&mut gs, b"\n");
      assert_eq!(gs.grid.cursor().row, 3);  // entered scroll region

      // Continue advancing within the region
      proc.advance(&mut gs, b"\n");
      assert_eq!(gs.grid.cursor().row, 4);
      proc.advance(&mut gs, b"\n");
      assert_eq!(gs.grid.cursor().row, 5);
      proc.advance(&mut gs, b"\n");
      assert_eq!(gs.grid.cursor().row, 6);
      proc.advance(&mut gs, b"\n");
      assert_eq!(gs.grid.cursor().row, 7);  // at scroll bottom

      // Next newline should scroll the region, cursor stays at row 7
      proc.advance(&mut gs, b"\n");
      assert_eq!(gs.grid.cursor().row, 7, "cursor should stay pinned at scroll bottom");
  }
  ```

  If `make_gs_with_size` does not exist, define it as a local helper following the `make_gs` pattern but with configurable rows/cols. Alternatively, use `make_gs()` if it already creates a grid large enough (check the existing helper).
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-vt -- newline_cursor_above_scroll_region</verify>
  <done>Test `newline_cursor_above_scroll_region_advances_into_region` passes. Cursor advances row-by-row from above the scroll region, enters it, and triggers a scroll when reaching the bottom.</done>
</task>

## Final Verification

```bash
cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-vt && cargo clippy -p arcterm-vt -- -D warnings
```

All `arcterm-vt` tests pass (existing + new regression tests). Clippy is clean.
