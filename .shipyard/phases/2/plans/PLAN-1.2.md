---
phase: terminal-fidelity
plan: "1.2"
wave: 1
dependencies: []
must_haves:
  - DEC private mode dispatch (? prefix) in csi_dispatch
  - DECSTBM, DCH, ICH, IL, DL, ECH, CHA, VPA, DSR, DA handlers
  - esc_dispatch handles DECKPAM, DECKPNM, DECSC, DECRC
  - Handler trait extended with new methods
  - Mode set/reset (h/l) dispatches to Grid mode flags
files_touched:
  - arcterm-vt/src/handler.rs
  - arcterm-vt/src/processor.rs
  - arcterm-vt/src/lib.rs
tdd: true
---

# PLAN-1.2 -- DEC Private Modes and Extended VT Sequences

## Goal

Extend the VT parser and Handler trait to support the full set of CSI and ESC
sequences required for vim, tmux, htop, and SSH sessions. This is the sequence
processing layer -- it translates byte sequences into Handler method calls. The
Grid implementations of these methods are provided by PLAN-1.1 (same wave, no
file overlap).

## Why This Must Come First

1. The processor currently ignores the `intermediates` parameter, so all DEC
   private mode sequences (CSI ? N h/l) are silently dropped.
2. `esc_dispatch` is a complete no-op -- DECKPAM, DECSC, DECRC never fire.
3. Critical CSI sequences like DECSTBM, DCH, ICH, IL, DL are missing.
4. Without these, vim/tmux/htop cannot render correctly regardless of how
   well the grid or renderer work.

## Tasks

<task id="1" files="arcterm-vt/src/handler.rs" tdd="false">
  <action>
  Extend the Handler trait with new method signatures (all with default no-op
  implementations) and add corresponding Grid implementations. New methods:

  Handler trait additions:
  - `fn set_mode(&mut self, _mode: u16, _private: bool) {}` -- DEC private set mode (CSI ? N h) and standard set mode (CSI N h)
  - `fn reset_mode(&mut self, _mode: u16, _private: bool) {}` -- DEC private reset mode (CSI ? N l) and standard reset mode (CSI N l)
  - `fn set_scroll_region(&mut self, _top: usize, _bottom: usize) {}` -- DECSTBM (CSI top;bottom r)
  - `fn save_cursor_position(&mut self) {}` -- DECSC (ESC 7)
  - `fn restore_cursor_position(&mut self) {}` -- DECRC (ESC 8)
  - `fn insert_lines(&mut self, _n: usize) {}` -- IL (CSI N L)
  - `fn delete_lines(&mut self, _n: usize) {}` -- DL (CSI N M)
  - `fn insert_chars(&mut self, _n: usize) {}` -- ICH (CSI N @)
  - `fn delete_chars(&mut self, _n: usize) {}` -- DCH (CSI N P)
  - `fn erase_chars(&mut self, _n: usize) {}` -- ECH (CSI N X)
  - `fn cursor_horizontal_absolute(&mut self, _col: usize) {}` -- CHA (CSI N G)
  - `fn cursor_vertical_absolute(&mut self, _row: usize) {}` -- VPA (CSI N d)
  - `fn device_status_report(&mut self, _n: u16) {}` -- DSR (CSI N n)
  - `fn device_attributes(&mut self) {}` -- DA (CSI c)
  - `fn set_keypad_application_mode(&mut self) {}` -- DECKPAM (ESC =)
  - `fn set_keypad_numeric_mode(&mut self) {}` -- DECKPNM (ESC >)

  Grid implementations for these methods:
  - `set_mode`/`reset_mode`: match on mode number when private=true:
    - 1 => app_cursor_keys
    - 7 => auto_wrap
    - 25 => cursor_visible
    - 47/1047 => enter/leave alt screen
    - 1049 => enter/leave alt screen + save/restore cursor
    - 2004 => bracketed_paste
    - 1000/1002/1003/1006 => store in modes (mouse reporting flags for future use)
  - `set_scroll_region` => delegate to Grid::set_scroll_region
  - `save_cursor_position` => delegate to Grid::save_cursor
  - `restore_cursor_position` => delegate to Grid::restore_cursor
  - `insert_lines` => delegate to Grid::insert_lines
  - `delete_lines` => delegate to Grid::delete_lines
  - `insert_chars` => delegate to Grid::insert_chars
  - `delete_chars` => delegate to Grid::delete_chars
  - `erase_chars` => delegate to Grid::erase_chars
  - `cursor_horizontal_absolute` => set_cursor with col=N, row unchanged
  - `cursor_vertical_absolute` => set_cursor with row=N, col unchanged
  - `device_status_report`: for n=6 (cursor position report), this needs to write
    back to the PTY. For now, log and no-op (requires write-back channel, deferred).
  - `device_attributes`: log and no-op for now.
  - `set_keypad_application_mode` => modes.app_keypad = true
  - `set_keypad_numeric_mode` => modes.app_keypad = false
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-vt</verify>
  <done>Handler trait compiles with all new methods. Grid impl compiles. All existing handler_tests and processor_tests still pass. No compilation errors in arcterm-app (downstream consumer).</done>
</task>

<task id="2" files="arcterm-vt/src/processor.rs" tdd="true">
  <action>
  Extend csi_dispatch and esc_dispatch in the Performer to dispatch the new sequences.

  csi_dispatch changes:
  1. Before the main `match action` block, check if `intermediates` contains `0x3F`
     (the `?` byte). If so, branch to a separate match for private mode handling:
     - action 'h' => for each param, call handler.set_mode(param, true)
     - action 'l' => for each param, call handler.reset_mode(param, true)
     - All other actions with '?' intermediate: ignore (return early)
  2. Add new match arms in the main (non-private) CSI dispatch:
     - 'h' => for each param, call handler.set_mode(param, false)
     - 'l' => for each param, call handler.reset_mode(param, false)
     - 'r' => DECSTBM: extract top (param 0, default 1) and bottom (param 1, default
       rows). Convert from 1-based to 0-based. Call handler.set_scroll_region(top, bottom).
       If no params, call handler.set_scroll_region(0, rows-1) to reset to full screen.
     - 'L' => IL: extract n (default 1), call handler.insert_lines(n)
     - 'M' => DL: extract n (default 1), call handler.delete_lines(n)
     - '@' => ICH: extract n (default 1), call handler.insert_chars(n)
     - 'P' => DCH: extract n (default 1), call handler.delete_chars(n)
     - 'X' => ECH: extract n (default 1), call handler.erase_chars(n)
     - 'G' => CHA: extract col (default 1), convert to 0-based, call
       handler.cursor_horizontal_absolute(col)
     - 'd' => VPA: extract row (default 1), convert to 0-based, call
       handler.cursor_vertical_absolute(row)
     - 'n' => DSR: extract mode, call handler.device_status_report(mode)
     - 'c' => DA: call handler.device_attributes()

  esc_dispatch changes:
  3. Replace the empty `esc_dispatch` with:
     - If intermediates is empty, match on byte:
       - 0x37 ('7') => handler.save_cursor_position()
       - 0x38 ('8') => handler.restore_cursor_position()
       - 0x3D ('=') => handler.set_keypad_application_mode()
       - 0x3E ('>') => handler.set_keypad_numeric_mode()
     - Otherwise ignore (intermediates present for other ESC sequences)

  4. Write tests covering:
     - CSI ? 25 l hides cursor (private mode dispatch works)
     - CSI ? 25 h shows cursor
     - CSI ? 1049 h enters alt screen
     - CSI ? 1049 l leaves alt screen
     - CSI 5;20 r sets scroll region
     - CSI r (no params) resets scroll region
     - ESC 7 / ESC 8 saves and restores cursor
     - CSI 1 P deletes one character
     - CSI 1 @ inserts one character
     - CSI 5 G moves cursor to column 5 (0-indexed: 4)
     - ESC = sets app keypad mode
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-vt</verify>
  <done>All new processor_tests pass: private mode sequences correctly dispatch through intermediates check, DECSTBM sets scroll region, ESC 7/8 saves/restores cursor, DCH/ICH/CHA work correctly. All existing tests still pass.</done>
</task>

<task id="3" files="arcterm-vt/src/lib.rs, arcterm-vt/src/processor.rs" tdd="true">
  <action>
  Add integration tests that exercise multi-sequence scenarios matching real-world
  application output. Add to the existing `processor_tests` module in lib.rs:

  1. **vim startup simulation**: Feed a byte sequence that includes:
     - CSI ? 1049 h (enter alt screen)
     - CSI 2 J (clear screen)
     - CSI 1;1 H (cursor home)
     - CSI 1;24 r (set scroll region rows 1-24, 1-based)
     - Some text writes
     - Verify: alt_screen_active is true, scroll region is set to (0, 23),
       cursor is at expected position, text appears in alt screen.

  2. **vim exit simulation**: Starting from the state above, feed:
     - CSI r (reset scroll region)
     - CSI ? 1049 l (leave alt screen)
     - Verify: alt_screen_active is false, original grid content is restored,
       scroll region is None.

  3. **htop status bar simulation**: Feed:
     - CSI 1;23 r (scroll region excludes last row for status bar)
     - Position cursor in scroll region, fill rows, newline at bottom margin
     - Verify: only rows within the scroll region scrolled, last row untouched.

  4. **Multiple mode set**: Feed CSI ? 1;25;2004 h (multiple params in one sequence)
     - Verify: app_cursor_keys=true, cursor_visible=true, bracketed_paste=true
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-vt -- --test-threads=1</verify>
  <done>All integration tests pass. vim enter/exit sequence correctly toggles alt screen and scroll region. htop scroll region test verifies status bar row is untouched during region scroll. Multi-mode set correctly applies all modes in a single sequence.</done>
</task>
