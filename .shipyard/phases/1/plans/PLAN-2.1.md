---
phase: foundation
plan: "2.1"
wave: 2
dependencies: ["1.1"]
must_haves:
  - vte Perform trait implemented with CSI dispatch for cursor movement, SGR colors, and erase sequences
  - Grid correctly mutates in response to VT byte sequences
  - All VT logic is unit-testable without GPU or PTY
files_touched:
  - arcterm-vt/src/lib.rs
  - arcterm-vt/src/handler.rs
  - arcterm-vt/src/processor.rs
  - arcterm-core/src/grid.rs
tdd: true
---

# Plan 2.1 -- VT Parser and Terminal Grid State Machine

**Wave 2** | Depends on: Plan 1.1 (arcterm-core types) | Parallel with: Plans 2.2, 2.3

## Goal

Implement the VT parsing layer that translates raw PTY output bytes into grid mutations. After this plan, feeding a byte stream through the processor produces a correctly populated Grid with cursor positioning, basic colors, and erase operations.

---

<task id="1" files="arcterm-vt/src/handler.rs, arcterm-core/src/grid.rs" tdd="true">
  <action>
    Define the `Handler` trait in `arcterm-vt/src/handler.rs` with semantic terminal operations. Then implement `Handler` for `arcterm_core::Grid` (this may require adding methods to Grid or implementing Handler in arcterm-vt using a newtype wrapper if orphan rules prevent it -- the pragmatic approach is to implement the handler methods directly on Grid in arcterm-core and have Handler be a trait in arcterm-vt that Grid satisfies).

    **Handler trait methods (all with default no-op impls):**
    ```
    fn put_char(&mut self, c: char)          // write char at cursor, advance cursor
    fn newline(&mut self)                     // move cursor down, scroll if at bottom
    fn carriage_return(&mut self)             // cursor to column 0
    fn backspace(&mut self)                   // cursor left one column (min 0)
    fn tab(&mut self)                         // advance to next tab stop (every 8 cols)
    fn bell(&mut self)                        // no-op for Phase 1
    fn set_cursor_pos(&mut self, row: usize, col: usize)  // CUP (CSI H)
    fn cursor_up(&mut self, n: usize)         // CUU
    fn cursor_down(&mut self, n: usize)       // CUD
    fn cursor_forward(&mut self, n: usize)    // CUF
    fn cursor_backward(&mut self, n: usize)   // CUB
    fn erase_in_display(&mut self, mode: u16) // ED: 0=below, 1=above, 2=all
    fn erase_in_line(&mut self, mode: u16)    // EL: 0=right, 1=left, 2=all
    fn set_sgr(&mut self, params: &[u16])     // SGR: color and attribute setting
    fn scroll_up(&mut self, n: usize)         // SU: scroll content up, new blank lines at bottom
    fn scroll_down(&mut self, n: usize)       // SD: scroll content down, new blank lines at top
    fn line_feed(&mut self)                   // LF (0x0A)
    fn set_title(&mut self, title: &str)      // OSC 0/2 window title (store but no-op render)
    ```

    **Grid extensions needed in arcterm-core/src/grid.rs:**
    - `Grid::scroll_up(&mut self, n: usize)` -- remove top n rows, append n blank rows at bottom.
    - `Grid::scroll_down(&mut self, n: usize)` -- remove bottom n rows, insert n blank rows at top.
    - `Grid::put_char_at_cursor(&mut self, c: char)` -- set cell at cursor, advance cursor right, wrap to next line if past last column, scroll if past last row.
    - `Grid::cursor(&self) -> CursorPos` -- return current cursor position.
    - `Grid::set_cursor(&mut self, pos: CursorPos)` -- set cursor with bounds clamping.
    - Store `current_attrs: CellAttrs` on Grid -- new characters inherit these attributes.
    - `Grid::set_attrs(&mut self, attrs: CellAttrs)` -- update current attrs.
    - `Grid::title: Option<String>` field for window title storage.

    **Tests (write first):**
    - `put_char` at (0,0) writes char, cursor advances to (0,1).
    - `put_char` at end of line wraps to next line.
    - `put_char` at bottom-right scrolls grid up.
    - `newline` from last row scrolls up.
    - `set_cursor_pos` clamps to grid bounds.
    - `erase_in_display(2)` clears all cells to spaces.
    - `erase_in_line(0)` clears from cursor to end of line.
    - `scroll_up(1)` moves row 1 content to row 0, bottom row is blank.
    - SGR parse: params [31] sets fg to Indexed(1), params [0] resets attrs.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-core --package arcterm-vt 2>&1 | tail -15</verify>
  <done>All Grid mutation tests pass. Handler trait is defined and Grid satisfies it. SGR parsing handles reset (0), bold (1), italic (3), underline (4), reverse (7), foreground colors (30-37, 90-97), background colors (40-47, 100-107), and 256-color (38;5;N / 48;5;N).</done>
</task>

<task id="2" files="arcterm-vt/src/processor.rs, arcterm-vt/src/lib.rs" tdd="true">
  <action>
    Implement `Processor` struct in `arcterm-vt/src/processor.rs` that wraps `vte::Parser` and bridges `vte::Perform` to the `Handler` trait.

    **Structure:**
    ```
    pub struct Processor {
        parser: vte::Parser,
    }

    impl Processor {
        pub fn new() -> Self { Self { parser: vte::Parser::new() } }
        pub fn advance<H: Handler>(&mut self, handler: &mut H, bytes: &[u8]);
    }
    ```

    The `advance` method needs to call `self.parser.advance()` with a performer that holds `&mut H`. Since vte's `advance` takes `&mut impl Perform`, create an internal `Performer<'a, H: Handler>` struct that holds `&'a mut H` and implements `vte::Perform`.

    **Perform implementation in Performer:**
    - `print(c: char)` -> `handler.put_char(c)`
    - `execute(byte)`:
      - 0x08 (BS) -> `handler.backspace()`
      - 0x09 (HT) -> `handler.tab()`
      - 0x0A (LF) -> `handler.line_feed()`
      - 0x0D (CR) -> `handler.carriage_return()`
      - 0x07 (BEL) -> `handler.bell()`
    - `csi_dispatch(params, intermediates, ignore, action)`:
      - If `ignore`, return immediately.
      - Match on `action`:
        - 'A' -> CUU: `handler.cursor_up(params[0] or 1)`
        - 'B' -> CUD: `handler.cursor_down(params[0] or 1)`
        - 'C' -> CUF: `handler.cursor_forward(params[0] or 1)`
        - 'D' -> CUB: `handler.cursor_backward(params[0] or 1)`
        - 'H' | 'f' -> CUP: `handler.set_cursor_pos(params[0]-1 or 0, params[1]-1 or 0)` (VT100 params are 1-based)
        - 'J' -> ED: `handler.erase_in_display(params[0] or 0)`
        - 'K' -> EL: `handler.erase_in_line(params[0] or 0)`
        - 'm' -> SGR: `handler.set_sgr(params)`
        - 'S' -> SU: `handler.scroll_up(params[0] or 1)`
        - 'T' -> SD: `handler.scroll_down(params[0] or 1)`
      - Extract params from `vte::Params` iterator -- each param is a `&[u16]` subparam list; for Phase 1, use the first subparam of each param.
    - `osc_dispatch(params, bell_terminated)`:
      - params[0] == b"0" or b"2" -> `handler.set_title(std::str::from_utf8(params[1]).unwrap_or(""))`
    - `esc_dispatch`, `hook`, `put`, `unhook` -> no-op for Phase 1.

    **`lib.rs`:** Re-export `Processor` and `Handler`.

    **Tests (write first):**
    - Feed `b"Hello"` -> grid has "Hello" at row 0 starting col 0.
    - Feed `b"\x1b[2J"` (erase display) -> all cells are spaces.
    - Feed `b"\x1b[5;10H"` -> cursor at (4, 9) (0-indexed).
    - Feed `b"\x1b[31mRed\x1b[0m"` -> "Red" cells have fg=Indexed(1), after reset attrs are default.
    - Feed `b"\x1b[2A"` -> cursor moves up 2 rows (clamped to 0).
    - Feed `b"\x1b[K"` -> erase from cursor to end of line.
    - Feed `b"\r\n"` -> carriage return then line feed.
    - Full integration: feed the output of `echo -e "\x1b[31mhello\x1b[0m world"` equivalent bytes and verify grid state.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-vt 2>&1 | tail -15</verify>
  <done>All Processor tests pass. Feeding VT100 byte sequences through `Processor::advance` correctly mutates a Grid. CSI cursor movement, SGR colors, erase sequences, and plain text all produce expected grid state.</done>
</task>

<task id="3" files="arcterm-vt/src/processor.rs" tdd="true">
  <action>
    Add tests for edge cases and sequences needed by real programs (`ls`, `vim`, `top`, `htop`):

    - **Line wrapping:** Feed 81 characters into an 80-column grid. Verify cursor wraps to row 1, col 0 after the 80th character, and the 81st character is at (1, 0).
    - **Scrolling:** Fill a 24-row grid completely, then feed another newline. Verify row 0 now contains what was row 1, and row 23 is blank.
    - **Tab stops:** Feed `\tX` and verify X is at column 8.
    - **256-color SGR:** Feed `\x1b[38;5;196m` (bright red 256-color) and verify fg = Indexed(196).
    - **RGB color SGR:** Feed `\x1b[38;2;255;128;0m` and verify fg = Rgb(255, 128, 0).
    - **Multiple SGR params:** Feed `\x1b[1;31;42m` and verify bold=true, fg=Indexed(1), bg=Indexed(2).
    - **CUP with defaults:** Feed `\x1b[H` (no params) and verify cursor at (0, 0).
    - **Erase below cursor:** Position cursor at (10, 0), feed `\x1b[J`, verify rows 10-23 are cleared but rows 0-9 are untouched.
    - **Backspace:** Position cursor at (0, 5), feed BS (0x08), verify cursor at (0, 4).

    These tests validate that the VT parser handles the sequences that `ls` (SGR colors, newlines), `vim` (cursor positioning, erase, alternate screen not needed yet), `top` (cursor home, erase, SGR), and `htop` (256-color SGR, cursor movement) will emit.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-vt 2>&1 | tail -15</verify>
  <done>All edge case tests pass. The VT parser correctly handles line wrapping, scrolling, tab stops, 256-color and RGB SGR, multi-param SGR, CUP defaults, erase modes, and backspace. Test count for arcterm-vt is at least 15.</done>
</task>
