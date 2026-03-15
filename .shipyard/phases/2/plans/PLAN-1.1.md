---
phase: terminal-fidelity
plan: "1.1"
wave: 1
dependencies: []
must_haves:
  - Scrollback ring buffer storing up to 10,000 rows that scroll off the top
  - Scroll regions (DECSTBM) for top/bottom margins
  - Alternate screen buffer with save/restore swap
  - Cursor save/restore (DECSC/DECRC)
  - Mode flags for cursor visibility, auto-wrap, app cursor keys, bracketed paste
  - Viewport offset for scrollback review
  - rows_for_viewport() method returns the visible slice for the renderer
files_touched:
  - arcterm-core/src/grid.rs
  - arcterm-core/src/cell.rs
  - arcterm-core/src/lib.rs
tdd: true
---

# PLAN-1.1 -- Scrollback Buffer, Scroll Regions, and Grid Mode State

## Goal

Transform Grid from a flat `Vec<Vec<Cell>>` with no memory into a scrollback-aware
grid with scroll regions, alternate screen buffer, cursor save/restore, and terminal
mode flags. This is the foundation for all Phase 2 features -- the VT handler, renderer,
and app layer all depend on these Grid capabilities.

## Why This Must Come First

1. `Grid.scroll_up()` currently calls `cells.drain(0..n)`, discarding rows forever.
   Scrollback requires capturing those rows before they are lost.
2. DECSTBM (scroll regions) is required by vim, tmux, and htop -- without it,
   scrolling affects the entire screen and status bars get destroyed.
3. Alternate screen buffer is required for vim, tmux, htop, and any full-screen TUI.
4. The renderer needs `rows_for_viewport()` to support scroll offset.

## Tasks

<task id="1" files="arcterm-core/src/grid.rs, arcterm-core/src/cell.rs" tdd="true">
  <action>
  Add scrollback storage and scroll regions to Grid. Specific changes:

  1. Add a `scrollback: VecDeque<Vec<Cell>>` field to Grid, with `max_scrollback: usize`
     (default 10,000).
  2. Add `scroll_region: Option<(usize, usize)>` field (top, bottom margins, 0-indexed
     inclusive). When None, the full screen is the scroll region.
  3. Modify `scroll_up()`: if scroll_region is None or covers the full screen, push
     drained rows onto `scrollback` (front of VecDeque) before removing them from cells.
     If scrollback exceeds max_scrollback, pop from the back. If scroll_region is set,
     only move rows within the region (do NOT push to scrollback -- region scrolls are
     in-place, matching xterm behavior).
  4. Modify `scroll_down()`: respect scroll_region bounds. No scrollback interaction
     (scroll_down inserts blank rows at the top of the region).
  5. Add `set_scroll_region(&mut self, top: usize, bottom: usize)` that validates
     bounds (top < bottom, both < rows) and stores Some((top, bottom)).
  6. Add `clear_scroll_region(&mut self)` that sets scroll_region to None.
  7. Modify `put_char_at_cursor()`: when the cursor is at the bottom of the scroll
     region (not the bottom of the screen), scroll only the region.
  8. Modify `newline()` in the Handler impl: when cursor is at the bottom margin of the
     scroll region, scroll only the region instead of the entire screen.
  9. Add `scrollback_len(&self) -> usize` accessor.
  10. Write tests FIRST covering:
      - scroll_up pushes row to scrollback
      - scrollback respects max capacity (overflow trims oldest)
      - scroll_up with scroll_region only affects rows within region
      - scroll_down with scroll_region only affects rows within region
      - put_char_at_cursor wraps within scroll region
      - newline at bottom of scroll region scrolls region only
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-core -- --test-threads=1</verify>
  <done>All existing grid tests still pass. New tests pass: scrollback stores rows, respects capacity limit. Scroll region tests pass: scroll_up/scroll_down/newline only affect rows within the set region. scrollback_len returns correct count.</done>
</task>

<task id="2" files="arcterm-core/src/grid.rs" tdd="true">
  <action>
  Add alternate screen buffer, cursor save/restore, mode flags, and viewport. Specific changes:

  1. Add mode flags to Grid as a struct `TermModes`:
     - `cursor_visible: bool` (default true)
     - `auto_wrap: bool` (default true)
     - `app_cursor_keys: bool` (default false)
     - `bracketed_paste: bool` (default false)
     - `alt_screen_active: bool` (default false)
     - `app_keypad: bool` (default false)
  2. Add `modes: TermModes` field to Grid.
  3. Add `saved_cursor: Option<(CursorPos, CellAttrs)>` field for DECSC/DECRC.
  4. Add `save_cursor(&mut self)` -- stores current cursor position and current_attrs.
  5. Add `restore_cursor(&mut self)` -- restores saved position and attrs (no-op if
     nothing saved).
  6. Add `alt_grid: Option<Box<Grid>>` field. When switching to alt screen, the current
     grid's cells/cursor/attrs are preserved and a fresh Grid is created as the active
     content. When switching back, the saved grid is restored.
  7. Add `enter_alt_screen(&mut self)` -- saves cursor, allocates alt grid with same
     size, sets alt_screen_active=true. The new alt grid starts with blank cells.
  8. Add `leave_alt_screen(&mut self)` -- restores saved cursor, swaps back to original
     grid content, sets alt_screen_active=false.
  9. Add `scroll_offset: usize` field (default 0). This is the viewport offset for
     scrollback review. 0 means showing the latest content.
  10. Add `rows_for_viewport(&self) -> Vec<&[Cell]>` (or equivalent iterator) that
      returns the rows currently visible considering scroll_offset. When scroll_offset=0,
      returns current cells. When scroll_offset > 0, returns a mix of scrollback rows
      and current rows.
  11. Modify `resize()` to also resize the alt_grid if present.
  12. Write tests FIRST covering:
      - save_cursor/restore_cursor round-trips position and attrs
      - enter_alt_screen/leave_alt_screen preserves original content
      - alt_screen starts with blank cells
      - rows_for_viewport with offset=0 returns current cells
      - rows_for_viewport with offset>0 returns scrollback + current mix
      - mode flags default values
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-core -- --test-threads=1</verify>
  <done>All tests pass. save_cursor/restore_cursor works. enter/leave_alt_screen preserves and restores original grid content. rows_for_viewport returns correct viewport for offset 0 and positive offsets. TermModes has correct defaults.</done>
</task>

<task id="3" files="arcterm-core/src/grid.rs, arcterm-core/src/lib.rs" tdd="false">
  <action>
  Wire up remaining Grid methods and ensure public API exports. Specific changes:

  1. Add `insert_lines(&mut self, n: usize)` -- inserts n blank lines at cursor row,
     pushing content down within the scroll region (or full screen). Lines that fall off
     the bottom of the region are discarded.
  2. Add `delete_lines(&mut self, n: usize)` -- deletes n lines at cursor row within
     the scroll region, pulling content up. Blank lines appear at the bottom of the region.
  3. Add `insert_chars(&mut self, n: usize)` -- inserts n blank cells at cursor column,
     shifting existing cells right. Cells that fall off the right edge are discarded.
  4. Add `delete_chars(&mut self, n: usize)` -- deletes n cells at cursor column,
     shifting remaining cells left. Blank cells appear at the right edge.
  5. Add `erase_chars(&mut self, n: usize)` -- replaces n cells starting at cursor with
     blanks (no shift, just overwrite).
  6. Update `arcterm-core/src/lib.rs` to re-export `TermModes` from the grid module.
  7. Verify all existing tests still pass (this task adds no new test file, but must
     not break anything).
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-core && cargo test --package arcterm-vt</verify>
  <done>All arcterm-core and arcterm-vt tests pass. insert_lines, delete_lines, insert_chars, delete_chars, erase_chars are public methods on Grid. TermModes is re-exported from arcterm-core.</done>
</task>
