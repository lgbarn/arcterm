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

### ISSUE-014 — usize underflow panic in `scroll_up` and `delete_lines` when `n == region_height` and `top`/`cur_row` == 0

- **Severity:** Critical
- **Source:** REVIEW-1.1
- **File:** `arcterm-core/src/grid.rs:182` (`scroll_up`), `arcterm-core/src/grid.rs:445` (`delete_lines`)
- **Description:** The ISSUE-010 in-place copy refactor computes `bottom - n` as a usize range upper bound in both `scroll_up` and `delete_lines`. When `n` is clamped to `region_height = bottom + 1 - top`, this evaluates to `top - 1`. When `top == 0` (the default VT100 scroll region), `0usize - 1` overflows: panics in debug mode; produces `usize::MAX` in release, which immediately causes an out-of-bounds index panic on `self.cells[row]`. This is a regression — the removed `Vec::remove`/`Vec::insert` loop handled all `n` values correctly. The symmetric methods (`scroll_down`, `insert_lines`) use the reverse-iterator pattern and are unaffected.
- **Remediation:** Replace `for row in top..=(bottom - n)` with `if let Some(last) = bottom.checked_sub(n) { for row in top..=last { ... } }` in both affected methods. Add regression tests: `scroll_up_full_region_with_top_at_zero` and `delete_lines_full_region_with_cursor_at_zero`.

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

### ISSUE-014 — Tool-not-found JSON does not escape `name` parameter

- **Severity:** Important
- **Source:** REVIEW-1.4
- **File:** `arcterm-plugin/src/manager.rs:379-382`
- **Description:** `call_tool()` returns `format!("{{\"error\":\"tool not found\",\"tool\":\"{}\"}}", name)` without JSON-escaping `name`. A `name` containing `"` or `\` produces malformed JSON, breaking any MCP caller that parses the response.
- **Remediation:** Use `serde_json::json!({"error": "tool not found", "tool": name}).to_string()` or a minimal escaping helper that replaces `\` → `\\` and `"` → `\"` before interpolation.

### ISSUE-015 — `validate_wasm_rejects_backslash` test exercises the `..` guard, not the `\` guard

- **Severity:** Important
- **Source:** REVIEW-1.4
- **File:** `arcterm-plugin/src/manifest.rs:399-403`
- **Description:** Test input `"..\\evil.wasm"` triggers the `contains("..")` check at line 134 before reaching the `contains('\\')` check at line 139. The assertion passes via the `..` branch, leaving the backslash-only validation path untested.
- **Remediation:** Change the test input to `"sub\\file.wasm"` (no `..`) so the backslash check at line 139 is exercised and the error message contains "backslash".

### ISSUE-016 — Epoch ticker OS thread is never terminated; leaks `Engine` Arc per `PluginRuntime`

- **Severity:** Important
- **Source:** REVIEW-1.4
- **File:** `arcterm-plugin/src/runtime.rs:28-32`
- **Description:** `std::thread::spawn` creates a detached loop with no exit condition. The thread holds `engine_clone` (an internal `Arc`), preventing the `Engine` from being dropped for the lifetime of the process. Each `PluginManager::new_with_dir()` call in tests spawns a new permanent thread; 7+ test functions each leak one thread + one engine.
- **Remediation:** Add a shutdown flag (`Arc<AtomicBool>`) checked in the loop; set it in `PluginRuntime::drop`. Alternatively use a channel: send on drop, break when received. In production the single-runtime model limits impact, but the pattern is fragile for tests.

### ISSUE-017 — Double-lock in `call_tool` has a TOCTOU window

- **Severity:** Suggestion
- **Source:** REVIEW-1.4
- **File:** `arcterm-plugin/src/manager.rs:368-376`
- **Description:** Lock is released after ownership check, then re-acquired for dispatch. Benign with static tool registration but could misdispatch if tool registration becomes dynamic.
- **Remediation:** Lock mutably from the start; check ownership and dispatch within the same critical section.

### ISSUE-018 — Canonicalize fallback weakens `load_from_dir` path escape check

- **Severity:** Suggestion
- **Source:** REVIEW-1.4
- **File:** `arcterm-plugin/src/manager.rs:250-252`
- **Description:** `wasm_path.canonicalize().unwrap_or(wasm_path.clone())` falls back to the raw path when the wasm file does not exist. Comparing a raw path against a canonicalized directory prefix produces a misleading "resolves outside the plugin directory" error rather than "file not found".
- **Remediation:** Propagate `canonicalize` errors with context ("plugin wasm not found") instead of silently falling back to the raw path.

### REVIEW-2.1-A — No test for cursor_col out-of-bounds in substitute_cursor_char

- **Severity:** Important
- **Source:** REVIEW-2.1
- **File:** `/Users/lgbarn/Personal/myterm/arcterm-render/src/text.rs:655`
- **Description:** `substitute_cursor_char` silently produces no substitution when `cursor_col` is `Some(col)` with `col >= row.len()`. This is safe today but leaves the out-of-bounds case undocumented. After a resize where `cursor.col` has not yet been clamped, or from a malformed VT sequence, the cursor will be invisible on a blank cell with no diagnostic. The intended behavior (silent no-op) is not tested.
- **Remediation:** Add test `cursor_col_out_of_bounds_no_panic` in `arcterm-render/src/text.rs mod tests`: construct a 5-cell row, call `substitute_cursor_char(&row, Some(10))`, assert all characters remain `' '`. This documents and guards the defensive behavior.

### REVIEW-2.1-B — `'\0'` guard in substitute_cursor_char is untested and potentially dead code

- **Severity:** Suggestion
- **Source:** REVIEW-2.1
- **File:** `/Users/lgbarn/Personal/myterm/arcterm-render/src/text.rs:662`
- **Description:** The condition `cell.c == ' ' || cell.c == '\0'` handles null-character cells, but `Cell::default()` always initializes `c` to `' '`, meaning the `'\0'` branch is never exercised by any test. If the branch is intentionally defensive (e.g., for future unsafe zero-initialized cells), it needs a test. If cells are always initialized to `' '`, the guard is dead code and may confuse future readers.
- **Remediation:** Either add test `cursor_on_null_cell_substitutes_block_glyph` (set `cell.c = '\0'`, verify U+2588 is returned), or remove the `|| cell.c == '\0'` clause with a comment noting that `Cell::default()` guarantees `' '`.

### ISSUE-019 — Window creation `.expect()` remains in `App::resumed()` after GPU init hardening

- **Severity:** Suggestion
- **Source:** REVIEW-1.2
- **File:** `arcterm-app/src/main.rs:1000`
- **Description:** `app.display.create_window(...).expect("failed to create window")` is the last panicking path in `resumed()`. GPU init is now graceful; window creation failure still produces a bare Rust backtrace with no user-facing message.
- **Remediation:** Wrap the window creation call in a match with `log::error!` + `event_loop.exit()` + `return`, using the same pattern applied to `Renderer::new()` in this plan.

### REVIEW-2.1-C — Dropped `JoinHandle` from `spawn_blocking` silences panics in blocking decode tasks

- **Severity:** Suggestion
- **Source:** REVIEW-2.1
- **File:** `/Users/lgbarn/Personal/myterm/arcterm-app/src/terminal.rs:108`
- **Description:** `tokio::task::spawn_blocking(...)` in `process_pty_output` returns a `JoinHandle<()>` that is immediately dropped. Tokio detaches the task (no cancel), so this is safe, but any panic inside the closure is silently swallowed with no observable signal from the PTY loop.
- **Remediation:** For Phase 5 image hardening consider collecting handles in a `Vec<JoinHandle<()>>` on `Terminal` and draining them (checking `.is_finished()`) in `about_to_wait`. For the current phase the `log::warn!` on decode error is sufficient coverage.

---

## Resolved

*(none yet)*
