# ISSUES

Non-blocking findings logged by the review agent. Resolve before the phase closes or carry forward with explicit justification.

---

## Open

### ISSUE-001 — `shutdown()` writer-drop mechanism is implicit and fragile

- **Severity:** Important
- **Source:** REVIEW-2.2
- **File:** `/Users/lgbarn/Personal/myterm/arcterm-pty/src/session.rs:153`
- **Description:** `shutdown()` replaces `self.writer` with `Box::new(io::sink())`. EOF reaches the shell only because the original `Box<dyn Write>` is dropped by `std::mem::replace`. If `portable_pty` internally clones the write handle (some platform backends do), dropping one `Box` does not close the underlying file descriptor and the shell never receives EOF.
- **Remediation:** Change the `writer` field to `Option<Box<dyn Write + Send>>`. In `shutdown()`, call `self.writer.take()` and let the returned value drop. In `write()`, return `Err(io::Error::new(io::ErrorKind::BrokenPipe, "session shut down"))` if the option is `None`. This also makes the `test_write_after_exit` test (required by the plan) straightforward to implement correctly.

### ISSUE-002 — Missing `request_redraw()` after keyboard input

- **Severity:** Important
- **Source:** REVIEW-3.1
- **File:** `/Users/lgbarn/Personal/myterm/arcterm-app/src/main.rs:205`
- **Description:** The `KeyboardInput` handler calls `terminal.write_input` but does not follow with `window.request_redraw()`. The spec (Task 2 integration section) explicitly requires this call to "immediately show the echo." Without it, typed characters in full-screen apps or during any gap before PTY response produce no immediate redraw, causing visible input lag.
- **Remediation:** Add `state.window.request_redraw();` immediately after `state.terminal.write_input(&bytes);` on line 205 of `arcterm-app/src/main.rs`.

### ISSUE-003 — Ctrl+\\ and Ctrl+] not handled in input translator

- **Severity:** Important
- **Source:** REVIEW-3.1
- **File:** `/Users/lgbarn/Personal/myterm/arcterm-app/src/input.rs:22-28`
- **Description:** The spec Priority 3 requires Ctrl+\\ → 0x1c (FS, SIGQUIT) and Ctrl+] → 0x1d (GS). The ctrl `Key::Character` arm only handles `is_ascii_alphabetic()` and `'['`, returning `None` for `\` and `]`. Ctrl+\\ (SIGQUIT) is a standard terminal signal key.
- **Remediation:** In the ctrl arm of `translate_key_event`, after the alphabetic check and `'['` check, add: `if lower == '\\' { return Some(vec![0x1c]); }` and `if lower == ']' { return Some(vec![0x1d]); }`.

### ISSUE-004 — PTY creation failure panics instead of graceful exit

- **Severity:** Important
- **Source:** REVIEW-3.1
- **File:** `/Users/lgbarn/Personal/myterm/arcterm-app/src/main.rs:88`
- **Description:** `Terminal::new(size).expect("failed to create PTY session")` panics on failure, producing a Rust backtrace rather than a user-facing error message. The spec (Task 3 error handling) requires logging the error and exiting with a meaningful message, not a panic.
- **Remediation:** Replace `.expect(...)` with `.unwrap_or_else(|e| { log::error!("Failed to create PTY session: {e}"); std::process::exit(1); })`, or propagate the error out of `resumed()` and handle it by calling `event_loop.exit()`.

### ISSUE-005 — Shell exit produces no visible in-window indicator

- **Severity:** Important
- **Source:** REVIEW-3.1
- **File:** `/Users/lgbarn/Personal/myterm/arcterm-app/src/main.rs:121-126`
- **Description:** When the PTY channel closes (shell exits), the code logs an info message and requests a final redraw, but the window displays the terminal's last frozen state with no indication the shell has exited. The spec (Task 3) requires displaying "Shell exited" in the window so the user understands the window is inert. The frozen window is indistinguishable from a hung process.
- **Remediation:** Add a `shell_exited: bool` field to `AppState`. Set it on `TryRecvError::Disconnected`. In the `RedrawRequested` handler, if `shell_exited` is true, write a "Shell exited — press any key to close" message into the grid or render it as a text overlay before calling `render_frame`.

### ISSUE-006 — Cursor invisible on blank cells due to text-only inverse-video approach

- **Severity:** Suggestion
- **Source:** REVIEW-3.1
- **File:** `/Users/lgbarn/Personal/myterm/arcterm-render/src/text.rs:112-117`
- **Description:** The cursor is rendered by recoloring the text glyph at the cursor cell to the cell's background palette color (inverse video). When the cursor rests on a space character (the most common case — empty grid cells and shell prompts park the cursor on a space), the glyph is whitespace and renders nothing, making the cursor completely invisible.
- **Remediation:** Implement a separate wgpu rectangle-draw pass for the cursor block in Phase 2. In the interim, consider rendering the cursor cell character as a `_` or block character (U+2588) when the cell is blank, so inverse-video produces a visible shape.

### ISSUE-007 — `set_scroll_region()` performs no bounds validation

- **Severity:** Important
- **Source:** REVIEW-1.1
- **File:** `/Users/lgbarn/Personal/myterm/arcterm-core/src/grid.rs:222-224`
- **Description:** The plan spec requires bounds validation (top < bottom, both < rows). The implementation stores any `(top, bottom)` pair without checking. If `bottom >= self.size.rows`, the `cells.remove(bottom)` call inside `scroll_up()` will panic at runtime with a Vec index out of bounds. If `top >= bottom`, scroll operations compute zero or negative region heights, producing silent no-ops or incorrect behavior.
- **Remediation:** Add validation before storing: `if top >= self.size.rows || bottom >= self.size.rows || top >= bottom { return; }`. At minimum clamp `bottom` to `self.size.rows.saturating_sub(1)` and assert `top < bottom`.

### ISSUE-008 — `resize()` does not resize `alt_grid` when present

- **Severity:** Important
- **Source:** REVIEW-1.1
- **File:** `/Users/lgbarn/Personal/myterm/arcterm-core/src/grid.rs:499-521`
- **Description:** The plan spec (Task 2, item 11) explicitly requires `resize()` to also resize `alt_grid` if present. The current implementation operates only on `self.cells` and ignores `self.alt_grid`. When the terminal is resized while on the alt screen, `leave_alt_screen()` will restore `cells` with the pre-resize row/col count, causing dimension mismatch against `self.size` and potential panics on any row-index access.
- **Remediation:** Add at the end of `resize()`: `if let Some(ref mut ag) = self.alt_grid { ag.resize(new_size); }`.

### ISSUE-009 — `scroll_offset` is an unvalidated public field with no setter

- **Severity:** Important
- **Source:** REVIEW-1.1
- **File:** `/Users/lgbarn/Personal/myterm/arcterm-core/src/grid.rs:85`
- **Description:** `scroll_offset` is a public `usize` field. `rows_for_viewport()` silently clamps it to `scrollback.len()` at call time, so callers that set `g.scroll_offset = 9999` get no feedback that the value is out of range. The mismatch between the stored field and the effective offset will cause confusion in scrollback-review UI logic (e.g., a scroll-up key press may appear to have no effect because the field is already past the cap).
- **Remediation:** Make `scroll_offset` private and expose `pub fn set_scroll_offset(&mut self, offset: usize) { self.scroll_offset = offset.min(self.scrollback.len()); }` and `pub fn scroll_offset(&self) -> usize { self.scroll_offset }`.

### ISSUE-010 — Scroll operations use O(n × rows) Vec::remove/Vec::insert per-row loop

- **Severity:** Suggestion
- **Source:** REVIEW-1.1
- **File:** `/Users/lgbarn/Personal/myterm/arcterm-core/src/grid.rs:170-173`, `grid.rs:204-208`, `grid.rs:375-379`, `grid.rs:400-404`
- **Description:** The partial-region scroll paths in `scroll_up()`, `scroll_down()`, `insert_lines()`, and `delete_lines()` call `Vec::remove` then `Vec::insert` in a loop, each of which is O(rows) due to element shifting. For a 1000-row grid scrolling 100 lines this is 100,000 shifts per call. The `GridState` version in `arcterm-vt/src/handler.rs:218-265` uses an in-place index-based copy loop which is O(rows × cols) total — substantially more efficient.
- **Remediation:** Replace the `for _ in 0..n { remove; insert; }` loops with the in-place copy pattern from `GridState::scroll_region_up/down()`: shift row contents by index within `[top..=bottom]`, then blank the tail rows.

### ISSUE-011 — `esc_dispatch` does not guard on empty intermediates

- **Severity:** Important
- **Source:** REVIEW-1.2
- **File:** `/Users/lgbarn/Personal/myterm/arcterm-vt/src/processor.rs:273`
- **Description:** The plan specifies that `esc_dispatch` should only dispatch when `intermediates` is empty ("Otherwise ignore"). The implementation names the parameter `_intermediates` and matches `byte` unconditionally. Byte `0x37` ('7') also appears as the final byte of SCS sequences such as `ESC ( 7` (select DEC special graphics character set). A terminal sending such a sequence will incorrectly trigger `save_cursor_position`, silently corrupting cursor state.
- **Remediation:** Rename `_intermediates` to `intermediates` and add `if !intermediates.is_empty() { return; }` at the top of `esc_dispatch` before the `match byte` block.

### ISSUE-012 — Modes 47, 1047, and mouse modes (1000/1002/1003/1006) absent from `set_mode`/`reset_mode`

- **Severity:** Important
- **Source:** REVIEW-1.2
- **File:** `/Users/lgbarn/Personal/myterm/arcterm-vt/src/handler.rs:452–505`
- **Description:** The plan's Task 1 action explicitly lists mode 47 and 1047 (enter/leave alt screen, the original sequences used before 1049 was standardised) and modes 1000/1002/1003/1006 (mouse reporting flags, "store in modes for future use") as required match arms. None are present. Mode 1047 is used by older applications and some tmux configurations; without it, those applications will not enter the alt screen. Mouse mode storage is required by the plan.
- **Remediation:** Add modes 47 and 1047 to `set_mode`/`reset_mode` using the same alt-screen enter/leave logic as 1049, minus cursor save/restore. Add `TermModes` fields `mouse_report_click`, `mouse_report_button`, `mouse_report_any`, `mouse_sgr_ext` (all `bool`) and set/clear them for 1000/1002/1003/1006 respectively.

### ISSUE-013 — `newline` scroll-region clamp is logically unreachable dead code with no test coverage for cursor-above-region case

- **Severity:** Important
- **Source:** REVIEW-1.2
- **File:** `/Users/lgbarn/Personal/myterm/arcterm-vt/src/handler.rs:295–302`
- **Description:** After advancing the cursor by one row (`cur_row + 1`), the code at line 296 checks `if self.grid.cursor().row < scroll_top` and clamps to `scroll_top`. This check is logically unreachable: the else branch is only entered when `cur_row < scroll_bottom`, and `cur_row + 1` is never less than `scroll_top` when the pre-advance position was already at or above it. The cursor-above-scroll-region case — where newline should advance the cursor toward the region without triggering a scroll — has no test, meaning silent mishandling would be undetected.
- **Remediation:** Remove the unreachable clamp block at lines 296–302. Add a unit test that positions the cursor above the scroll region and verifies successive `newline` calls advance it row-by-row until it enters the region, at which point a further newline triggers a scroll only within the region.

---

## Resolved

*(none yet)*
