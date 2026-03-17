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

### REVIEW-1.1-A — `StructuredContentAccumulator` buffer population is unverified for the Wave 2 migration path

- **Severity:** Important
- **Source:** REVIEW-1.1
- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/osc7770.rs:32–41`
- **Description:** The local `StructuredContentAccumulator` in `osc7770.rs` is a data-transfer object with no mutation methods. Its `buffer` field is currently populated by `arcterm_vt::GridState`, which copies into it via the old accumulator. When Wave 2 replaces `GridState` with the alacritty-based engine, the path that writes characters into `buffer` must be explicitly implemented in the new state machine. If this is overlooked, `take_completed_blocks()` will return accumulators with empty buffers — blocks with correct `content_type` but no text — and no compile error will flag it.
- **Remediation:** In the Wave 2 plan (PLAN-2.1), add an explicit acceptance criterion: write a test that feeds a synthetic OSC 7770 sequence through the new engine and asserts `buffer` is non-empty on the returned accumulator. This closes the gap before the `arcterm_vt` dependency is removed.

### ISSUE-020 — ST-terminated non-intercepted OSC sequences reconstructed with BEL terminator

- **Severity:** Important
- **Source:** REVIEW-1.2
- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/prefilter.rs:269–276` (`reconstruct_osc_passthrough`)
- **Description:** `reconstruct_osc_passthrough` always emits `0x07` (BEL) as the terminator regardless of whether the original sequence used BEL or ST (`ESC \`). Most terminal OSC handlers accept both terminators, but the substitution is not universally safe. Applications that embed OSC passthrough within DCS strings (e.g., tmux passthrough), or any OSC consumer that strictly requires ST, will receive a malformed sequence. The doc comment asserts "The terminal engine accepts both" without qualification.
- **Remediation:** Add a `last_osc_terminator: u8` field to `PreFilter` (initialized to `0x07`). Set it to `0x07` when BEL is seen in `InOsc`, and record the ST bytes when `ESC \` is seen in `InOscPendingEsc`. In `reconstruct_osc_passthrough`, emit the original terminator rather than hardcoded BEL (for ST, push both `0x1b` and `b'\\'`).

### ISSUE-021 — Missing `test_empty_input` test required by plan acceptance criteria

- **Severity:** Important
- **Source:** REVIEW-1.2
- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/prefilter.rs`, tests module (line 322)
- **Description:** Task 2 acceptance criteria explicitly requires "Edge case: empty input produces empty output." No such test exists in the 14-test suite. The behavior is correct (the loop is a no-op on an empty slice), but the spec test is absent.
- **Remediation:** Add `fn test_empty_input()` that calls `run(b"")` and asserts all four output fields are empty.

### ISSUE-022 — `PreFilterOutput::new()` is private; `Default` not implemented

- **Severity:** Important
- **Source:** REVIEW-1.2
- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/prefilter.rs:53`
- **Description:** `PreFilterOutput::new()` is private and `Default` is not derived. Wave 2 integration code in `terminal.rs` that needs to construct or merge `PreFilterOutput` values cannot do so without accessing internals. All four fields are `Vec` types that implement `Default`, so this is a trivial derive gap.
- **Remediation:** Add `Default` to the derive list on line 40: `#[derive(Debug, Clone, PartialEq, Eq, Default)]`.

### ISSUE-023 — No test for ESC at end of buffer (PendingEsc split across call boundary)

- **Severity:** Suggestion
- **Source:** REVIEW-1.2
- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/prefilter.rs`, tests module
- **Description:** The plan mentions "ESC at end of buffer" as an edge case. No test exercises a split where the final byte of the first call is `ESC` and the introducer byte arrives in the second call. The `PendingEsc` state is preserved correctly by the implementation, but no test proves it.
- **Remediation:** Add `fn test_esc_at_buffer_boundary()` using `run_split(b"hello\x1b", b"[0m")` and assert `passthrough == b"hello\x1b[0m"`.

### REVIEW-2.1-D — `dispatch_osc7770` thread-local accumulator leaks across panes

- **Severity:** Important
- **Source:** REVIEW-2.1
- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/terminal.rs:856–911`
- **Description:** `dispatch_osc7770` stores the in-progress accumulator in a `thread_local! { static ACTIVE }`. It is called from the main thread for all panes. In a multi-pane session, interleaved OSC 7770 events from different panes will corrupt each other's accumulator state (e.g., pane B's `"end"` event pops pane A's open accumulator).
- **Remediation:** Move the accumulator field into `Terminal` as `active_osc7770: Option<StructuredContentAccumulator>`. Change `dispatch_osc7770` to take `active: &mut Option<StructuredContentAccumulator>` instead of using `ACTIVE`. Call site in `has_wakeup` passes `&mut self.active_osc7770`.

### REVIEW-2.1-E — `strip_ansi` corrupts multi-byte UTF-8 characters

- **Severity:** Important
- **Source:** REVIEW-2.1
- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/terminal.rs:959`
- **Description:** `result.push(bytes[i] as char)` in the non-escape byte branch casts each raw byte individually to `char`. UTF-8 continuation bytes (0x80–0xBF) produce garbage `char` values rather than the correct Unicode character. Any OSC 7770 block containing non-ASCII text (e.g., UTF-8 source code with non-ASCII identifiers, accented comments) will have its content corrupted in `acc.buffer`.
- **Remediation:** Replace the byte push with a char-level decode: `if let Some(ch) = std::str::from_utf8(&bytes[i..]).ok().and_then(|s| s.chars().next()) { result.push(ch); i += ch.len_utf8(); } else { i += 1; }`. See REVIEW-2.1 for full replacement snippet.

### REVIEW-2.1-F — `splitn(3, ';')` drops OSC 7770 attributes beyond the third semicolon

- **Severity:** Suggestion
- **Source:** REVIEW-2.1
- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/terminal.rs:863`
- **Description:** `splitn(3, ';')` caps the split at 3 parts. A params string with 4+ attributes (e.g., `"start;type=code;lang=rust;id=42"`) has its fourth and subsequent attributes merged into the third element's value, silently dropping all attributes after the third.
- **Remediation:** Split the verb from the rest with `splitn(2, ';')`, then split the rest with `split(';')` for unlimited attribute parsing.

### REVIEW-2.1-G — `write_input` silently discards data when the write channel is full

- **Severity:** Important
- **Source:** REVIEW-2.1
- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/terminal.rs:459`
- **Description:** `try_send` on a full 16-slot sync channel logs a warning and drops the payload. For user keyboard input, this means keystrokes can be silently lost under load (e.g., fast paste, neovim macro replay). No backpressure or retry logic exists.
- **Remediation:** For the keyboard input path in `main.rs`, call a blocking `write_input_blocking` variant (using `send` instead of `try_send`). Reserve `try_send` for automated PTY reply paths where dropping under load is acceptable.

### REVIEW-2.1-H — Reader thread EOF path does not send a final wakeup signal

- **Severity:** Suggestion
- **Source:** REVIEW-2.1
- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/terminal.rs:352–354`
- **Description:** On PTY EOF (shell exits cleanly without a `ChildExit` event), the reader thread breaks without sending a wakeup. If `has_exited()` also returns false (exit_code is None), the pane remains open with no drain. In practice `ChildExit` fires before EOF in most cases, but the edge is present.
- **Remediation:** Clone `wakeup_tx` into the reader thread closure. Before the `Ok(0) => break`, add `let _ = wakeup_tx_clone.send(())`.

### ISSUE-024 — `child_pid()` returns infallible `Some(u32)` but is typed `Option<u32>`

- **Severity:** Important
- **Source:** REVIEW-4.1
- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/terminal.rs:655–656`
- **Description:** `child_pid()` returns `Some(self.child_pid)` unconditionally — `child_pid` is a `u32` field set at construction time and can never be absent. The `Option<u32>` return type was designed for the old architecture where PID capture might fail. All callers pattern-match or call `.is_some()` unnecessarily. The misleading type will cause future maintainers to add defensive `None` handling for a case that cannot occur.
- **Remediation:** Change the return type to `u32` and update all call sites (`main.rs:2747–2750`, integration test `terminal_creates_pty_and_reports_pid`). Alternatively, if retaining `Option<u32>` for forward compatibility, document prominently on the method that it currently always returns `Some`.

### ISSUE-025 — `push_output_line` and `set_command` unwired; output ring is always empty at runtime

- **Severity:** Important
- **Source:** REVIEW-4.1
- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/context.rs:77–78`, `:87–88`
- **Description:** `push_output_line` and `set_command` have no callers in `main.rs`. The `output_ring` VecDeque and `last_command` field of every `PaneContext` are permanently empty at runtime. As a result, `error_context()` always returns `ErrorContext { output_tail: [], command: "" }`, and `format_error_osc7770` produces blocks with no content. The `#[allow(dead_code)]` annotations suppress the compiler warning that would otherwise flag this wiring gap.
- **Remediation:** Wire `set_command` to the OSC 133 B handler and `push_output_line` to the PTY output processing loop in `main.rs`. If deferred to a future phase, replace `#[allow(dead_code)]` with a `// TODO(phase-N): wire to OSC 133 B handler` comment and open a tracking item so the gap is not forgotten.

### REVIEW-3.1-A — `shape_row_into_buffer` drops bold and italic terminal attributes

- **Severity:** Important
- **Source:** REVIEW-3.1
- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-render/src/text.rs:666–698`
- **Description:** `shape_row_into_buffer` builds `span_strings: Vec<(String, Color)>` using only the foreground color per cell. `cell.bold` and `cell.italic` are never applied to the glyphon `Attrs`. All bold and italic terminal text renders at normal weight/upright, visually indistinguishable from plain text. The attributes are correctly stored in `SnapshotCell` and hashed in `hash_row` but are silently discarded at the shaping stage.
- **Remediation:** Change the span element type from `(String, Color)` to `(String, Attrs)`. Build `Attrs` per cell including weight and style:
  ```rust
  let mut attrs = Attrs::new().family(Family::Monospace).color(fg);
  if cell.bold   { attrs = attrs.weight(Weight::BOLD); }
  if cell.italic { attrs = attrs.style(Style::Italic); }
  (s, attrs)
  ```
  Pass `(item.0.as_str(), item.1.clone())` into `buf.set_rich_text`.

### REVIEW-3.1-B — `reset_frame` discards `pane_buffer_pool` allocations every frame

- **Severity:** Important
- **Source:** REVIEW-3.1
- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-render/src/text.rs:129–135`
- **Description:** `reset_frame` calls `self.pane_buffer_pool.truncate(0)`, dropping all `Vec<Buffer>` entries despite the comment stating "pane_buffer_pool already holds them; just clear the metadata." The intent to reuse allocations is not realized. Every frame reallocates all `Buffer` objects for all panes. For a 2-pane layout at 24 rows each, this is ~48 `Buffer` allocations per frame (~2,880/second at 60fps). `Buffer` allocates internal glyph storage on construction.
- **Remediation:** Remove `self.pane_buffer_pool.truncate(0)` from `reset_frame`. Keep only `self.pane_slots.clear()`. The pool will grow to the max pane count seen and stabilize there. The existing `if slot_idx >= self.pane_buffer_pool.len()` push in `prepare_grid_at` correctly handles growth; the lack of shrink on reset is the desired behavior.

### REVIEW-3.1-C — `prepare_grid_at` re-shapes all rows every frame; dirty-row optimization absent from hot path

- **Severity:** Important
- **Source:** REVIEW-3.1
- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-render/src/text.rs:229–280`
- **Description:** `prepare_grid` (single-pane path) uses `row_hashes` to skip re-shaping unchanged rows. `prepare_grid_at` (multi-pane path, the actual production path) re-shapes every row unconditionally every frame. For applications with largely static output (prompts, man pages, editor buffers between keystrokes), nearly all 80-col × 24-row cell iterations are wasted. This is a performance gap relative to the prior architecture's behavior.
- **Remediation:** Add per-pane row hash vecs in `AppState` (one `Vec<u64>` per `PaneId`), pass them into `prepare_grid_at` as `row_hashes: &mut Vec<u64>`, and apply the same `hash_row` skip logic as `prepare_grid`. Clear the hash vec for a pane on resize or palette change.

### REVIEW-3.1-D — `RenderSnapshot::row()` panics on out-of-bounds row_idx; asymmetric with `cell()`

- **Severity:** Suggestion
- **Source:** REVIEW-3.1
- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-render/src/snapshot.rs:115–118`
- **Description:** `row()` slices `&self.cells[start..start + self.cols]` with no bounds check; an out-of-bounds `row_idx` panics. `cell()` at line 105 has an explicit bounds check returning `Option`. All current callers are bounded by `0..snapshot.rows`, but the asymmetry between the two helpers is a latent hazard.
- **Remediation:** Add `assert!(row_idx < self.rows, "row_idx {row_idx} >= rows {}", self.rows);` at the top of `row()`, or change the return type to `Option<&[SnapshotCell]>` consistent with `cell()`.

### REVIEW-3.1-E — Shell-exited banner mutates snapshot cells via raw public field access

- **Severity:** Suggestion
- **Source:** REVIEW-3.1
- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/main.rs:2084–2097`
- **Description:** The exit banner is written by directly indexing `display.cells[row_start + col]`, coupling `main.rs` to `RenderSnapshot`'s internal `Vec<SnapshotCell>` layout. A future refactor that makes `cells` `pub(crate)` or changes storage would silently break this path.
- **Remediation:** Add `RenderSnapshot::write_row_text(row: usize, text: &str, fg: SnapshotColor, bg: SnapshotColor, bold: bool)` to `snapshot.rs` to encapsulate the row-overwrite logic. Callers in `main.rs` use the named API rather than raw index arithmetic.

### REVIEW-01-A — `submit_text_areas` zip length mismatch is silent and unexplained

- **Severity:** Important
- **Source:** REVIEW-01
- **File:** `/Users/lgbarn/Personal/arcterm/arcterm-render/src/text.rs:308`
- **Description:** `self.pane_buffer_pool.iter().zip(self.pane_slots.iter())` silently truncates to the shorter of the two slices. Since Task 1 removed `truncate(0)` from `reset_frame`, the pool is now longer than `pane_slots` between frames (pool grows monotonically; slots reset each frame). The behavior is correct — `zip` stops at `pane_slots.len()` — but there is no comment explaining this intentional length divergence. A reviewer or future maintainer will reasonably suspect a bug.
- **Remediation:** Add a comment above the zip on line 308: `// pane_buffer_pool may be longer than pane_slots (pool grows monotonically; slots reset each frame). zip stops at pane_slots.len(), which is correct.`

### REVIEW-01-B — `pane_row_hashes` is `pub` on `Renderer`, bypassing invalidation invariant

- **Severity:** Important
- **Source:** REVIEW-01
- **File:** `/Users/lgbarn/Personal/arcterm/arcterm-render/src/renderer.rs:79`
- **Description:** `pub pane_row_hashes: HashMap<usize, Vec<u64>>` exposes the dirty-row cache as a fully public field. External callers can insert, remove, or modify entries without going through `resize()` or `set_palette()`, which are the only two code paths that guarantee cache invalidation on layout or color changes. Stale hashes after a palette swap would cause rows to render with wrong colors until cell content happens to change.
- **Remediation:** Change `pub pane_row_hashes` to `pub(crate) pane_row_hashes`. If crates outside `arcterm-render` need to observe or clear the cache, expose a `clear_row_hashes()` method on `Renderer` instead of the raw field.

### ISSUE-026 — Residual `unwrap_or` fallback on `dir_canonical` in `load_from_dir`

- **Severity:** Important
- **Source:** REVIEW-1.2
- **File:** `arcterm-plugin/src/manager.rs:256`
- **Description:** `let dir_canonical = dir.canonicalize().unwrap_or(dir.to_path_buf())` silently falls back to the raw path if the plugin directory cannot be canonicalized (e.g., a permission error). After ISSUE-018 fixed `wasm_canonical` to propagate errors explicitly, this line is the remaining `unwrap_or` in the same path. If `dir.canonicalize()` fails for a permission reason while `wasm_path.canonicalize()` succeeds, the subsequent `!wasm_canonical.starts_with(&dir_canonical)` comparison compares a canonical path against a raw path, producing a false positive "resolves outside the plugin directory" error that obscures the actual cause.
- **Remediation:** Apply the same explicit error pattern: `let dir_canonical = dir.canonicalize().map_err(|e| anyhow::anyhow!("cannot canonicalize plugin directory '{}': {e}", dir.display()))?;`.

### REVIEW-02-A — Background-tab pane exit leaves stale Leaf node in layout tree

- **Severity:** Important
- **Source:** REVIEW-02
- **File:** `/Users/lgbarn/Personal/arcterm/arcterm-app/src/main.rs:1665,1668`
- **Description:** The pane exit removal loop captures `let active = state.tab_manager.active` once before iterating `closed_panes`. `state.tab_layouts[active].close(id)` is called for every exited pane, but if the pane belongs to a background tab (a tab other than the one currently being displayed), `close()` returns `None` — the pane is not in the active tab's tree — and the stale `Leaf` node is left in the background tab's layout indefinitely. In the current single-tab-workflow the bug is dormant, but any multi-tab usage where a pane exits while the user is viewing a different tab will produce a permanently stale layout for that background tab.
- **Remediation:** Add a helper `fn tab_index_for_pane(&self, id: PaneId) -> Option<usize>` on `AppState` that iterates `self.tab_layouts.iter().enumerate()` calling `all_pane_ids()` on each, returning the index of the tab whose layout contains `id`. In the removal loop, call this helper to route `close(id)` to the correct tab index rather than always using `active`.

### REVIEW-T2-A — `AppMenu::new()` panics on submenu construction failure

- **Severity:** Important
- **Source:** REVIEW Task 2 (menu.rs)
- **File:** `/Users/lgbarn/Personal/arcterm/arcterm-app/src/menu.rs:100-101, 177-178, 227-228, 398-399, 421-422, 427-428`
- **Description:** All six `append_items` calls use `.expect("... append_items failed")`. On a platform where `muda` cannot construct the menu bar (unsupported compositor, Wayland without XDG decoration, headless test runner), the process panics with no graceful degradation. Menu construction happens in `resumed()` which should propagate failure back to the event loop rather than aborting.
- **Remediation:** Change `AppMenu::new()` to return `Result<Self, muda::Error>`, replace all `.expect()` calls with `?`, and update the call site in Task 3 to handle the error with `log::error!` + `event_loop.exit()`, consistent with the pattern established for PTY and GPU init failures.

### REVIEW-T2-B — `id_map` HashMap not pre-allocated

- **Severity:** Important
- **Source:** REVIEW Task 2 (menu.rs)
- **File:** `/Users/lgbarn/Personal/arcterm/arcterm-app/src/menu.rs:37`
- **Description:** `HashMap::new()` is used. The map will receive exactly 34 insertions (6 Shell + 8 Edit + 6 View + 14 Window + 3 Help), causing the default HashMap to rehash twice during construction.
- **Remediation:** Change to `HashMap::with_capacity(40)` to eliminate all rehash allocations during `AppMenu::new()`.

### REVIEW-T4-A — `execute_key_action` silently discards `DispatchOutcome::Redraw`

- **Severity:** Important
- **Source:** REVIEW Task 4 (dispatch_action)
- **File:** `/Users/lgbarn/Personal/arcterm/arcterm-app/src/main.rs:3265-3269`
- **Description:** `execute_key_action` merges the `Redraw` and `None` arms with `DispatchOutcome::Redraw | DispatchOutcome::None => {}`. The call site at line 2970 issues an unconditional `state.window.request_redraw()` immediately after, which masks the bug for the currently mapped palette actions. However, any future palette action whose `dispatch_action` returns `DispatchOutcome::None` (e.g. a no-op stub) will still trigger an unnecessary redraw, and conversely if a future caller calls `execute_key_action` without the trailing unconditional redraw, a genuine `Redraw` outcome will be silently dropped. The contract is ambiguous: the comment says "caller is responsible for calling request_redraw()" but the function signature gives no indication of this obligation.
- **Remediation:** Return `DispatchOutcome` from `execute_key_action` and let the call site branch on it explicitly: `if execute_key_action(state, event_loop, key_action) == DispatchOutcome::Redraw { state.window.request_redraw(); }`. Alternatively, handle `Redraw` inside the function and remove the unconditional trailing redraw at the call site, making the function self-contained.

### REVIEW-T4-B — Keyboard handler `other` arm holds an extra implicit borrow risk via `focused_plugin_id`

- **Severity:** Suggestion
- **Source:** REVIEW Task 4 (dispatch_action)
- **File:** `/Users/lgbarn/Personal/arcterm/arcterm-app/src/main.rs:3161-3244`
- **Description:** `focused_plugin_id` is computed by calling a nested function over `state.active_layout()` which borrows `state`. This borrow ends before `match action`, so the borrow checker is satisfied today. However the `find_plugin_id` closure is defined inline inside the `WindowEvent::KeyboardInput` arm each time a key is pressed, adding a non-trivial amount of code to a hot path. This is a readability and potential maintenance burden — future additions to the match arm (particularly any that also borrow `state` immutably) risk confusing the borrow lifetimes.
- **Remediation:** Hoist `find_plugin_id` to a module-level free function (`fn find_plugin_id(node: &PaneNode, target: PaneId) -> Option<String>`) or add it as an associated function on `PaneNode`. This keeps the hot path clean and makes the helper independently testable.

### REVIEW-T5-A — `ClearScrollback` sends `\x1b[3J` to PTY stdin instead of the terminal emulator

- **Severity:** Important
- **Source:** REVIEW Task 5 (menu-only action handlers)
- **File:** `/Users/lgbarn/Personal/arcterm/arcterm-app/src/main.rs:1479`
- **Description:** `KeyAction::ClearScrollback` calls `terminal.write_input(b"\x1b[3J")`. `write_input` sends bytes to the PTY writer thread — the shell's stdin — not to the terminal emulator's output parser. The shell (bash/zsh) will receive the raw ESC bytes as keyboard input and misinterpret them. The emulator never sees `\x1b[3J` and the scrollback is never cleared. The correct API to clear history is `terminal.with_term_mut(|t| t.grid_mut().clear_history())` which calls `Grid::clear_history()` from alacritty_terminal 0.25.1 directly.
- **Remediation:** Replace `terminal.write_input(b"\x1b[3J")` with `terminal.with_term_mut(|t| t.grid_mut().clear_history())`. Remove the `write_input` call entirely.

### REVIEW-T5-B — `ResetTerminal` sends `\x1bc` to PTY stdin instead of the terminal emulator

- **Severity:** Important
- **Source:** REVIEW Task 5 (menu-only action handlers)
- **File:** `/Users/lgbarn/Personal/arcterm/arcterm-app/src/main.rs:1552`
- **Description:** `KeyAction::ResetTerminal` calls `terminal.write_input(b"\x1bc")`. Same mechanism as REVIEW-T5-A: `write_input` routes bytes to the shell's stdin. The RIS sequence `\x1bc` will be received by the shell as literal keyboard input (e.g., bash treats `ESC c` as `alt+c` → capitalize-word in readline), not as an emulator reset. The correct approach is to call `terminal.with_term_mut(|t| t.reset_state())` if that API exists, or inject the sequence into the PTY's output/master side. Check the alacritty_terminal `Term` API for a `reset` or `reset_state` method.
- **Remediation:** Investigate `alacritty_terminal::term::Term::reset_state()` or equivalent in 0.25.1. If available, call `terminal.with_term_mut(|t| t.reset_state())`. If not, expose a `reset()` helper in `Terminal` that locks the term and calls the appropriate reset API directly rather than routing through `write_input`.

### REVIEW-T5-C — `ShowDebugInfo` writes debug text to shell stdin via `write_input`

- **Severity:** Important
- **Source:** REVIEW Task 5 (menu-only action handlers)
- **File:** `/Users/lgbarn/Personal/arcterm/arcterm-app/src/main.rs:1571-1581`
- **Description:** `KeyAction::ShowDebugInfo` calls `terminal.write_input(info.as_bytes())` to "write to terminal". This sends the multi-line debug string as raw input to whatever program is running in the focused pane. In an interactive shell this corrupts the readline buffer; in a running program (nvim, less) it injects bytes as keystrokes. The spec says "writes to terminal" intending the displayed output, not the program's stdin. The `log::info!` call before it already surfaces the information to the log.
- **Remediation:** Remove the `write_input` call. Either: (a) keep only the `log::info!` for now and note this as a follow-up for a proper overlay, or (b) implement a transient overlay `DebugInfoOverlay` state on `AppState` that is rendered in the next frame and dismissed on any key, consistent with the palette/workspace-switcher overlay pattern already in use. The spec's plan note says "For now just log it. A proper overlay can come later." — the remediation should be to remove the `write_input` call and leave only the log line.

### REVIEW-T5-D — `IncreaseFontSize` has no upper-bound clamp and font change is not propagated to renderer

- **Severity:** Important
- **Source:** REVIEW Task 5 (menu-only action handlers)
- **File:** `/Users/lgbarn/Personal/arcterm/arcterm-app/src/main.rs:1484-1487`
- **Description:** Two related gaps: (1) `IncreaseFontSize` adds 1.0 to `config.font_size` with no upper bound. A user pressing the increase menu item many times can set `font_size` to arbitrarily large values. `DecreaseFontSize` correctly clamps to `min(6.0)` but `IncreaseFontSize` has no symmetric `max`. (2) None of the three font-size handlers call any renderer method to apply the change. `config.font_size` is mutated but the `Renderer` continues using the size it was initialized with. The plan acknowledges `renderer.update_font_size()` does not exist and defers it, but without propagation the font size menu items do nothing visible to the user.
- **Remediation:** (1) Add `.min(72.0)` clamp to `IncreaseFontSize`: `self.config.font_size = (self.config.font_size + 1.0).min(72.0)`. (2) Add a `Renderer::set_font_size(size: f32)` method in `arcterm-render` that re-creates the `TextAtlas` with the new metrics and re-measures cell size, then call it from all three font-size handlers.

### REVIEW-T5-E — `PaneNode::equalize()` has no unit test

- **Severity:** Suggestion
- **Source:** REVIEW Task 5 (menu-only action handlers)
- **File:** `/Users/lgbarn/Personal/arcterm/arcterm-app/src/layout.rs:474-488`
- **Description:** The `equalize()` method is a new public API with no test coverage. The existing layout test suite in `layout.rs` covers split, close, focus, resize, and rect operations — all methods that `equalize` is peer to. A tree with nested HSplit/VSplit nodes at non-0.5 ratios is the primary test scenario; both the recursive normalization and the leaf/PluginPane base case should be covered.
- **Remediation:** Add two tests in `layout.rs mod tests`: (1) `equalize_flat_tree_sets_all_ratios_to_half` — build a three-pane HSplit (left=Leaf, right=VSplit(Leaf, Leaf)) with ratios 0.3/0.7, call `equalize()`, assert all ratios are 0.5. (2) `equalize_leaf_is_noop` — call `equalize()` on a `Leaf` node and assert it does not panic.

### ISSUE-027 — CloseTab missing pane-state cleanup (nvim_states, ai_states, pane_contexts)
- **Severity:** high
- **Source:** simplifier (phase 15)
- **Date:** 2026-03-16
- **File:** `arcterm-app/src/main.rs:1287-1306`
- **Description:** The `KeyAction::CloseTab` handler removes panes from `self.panes` and `self.image_channels` but does not remove entries from `self.nvim_states`, `self.ai_states`, or `self.pane_contexts`. The `ClosePane` arms both perform all five removals. The gap exists because the three cleanup loops were written independently (copy-paste drift).
- **Remediation:** Extract a `fn remove_pane_resources(&mut self, id: PaneId)` method that performs all five map removals plus the `last_ai_pane` null-out. Replace all three loop bodies with calls to this method.

### ISSUE-028 — DispatchOutcome match arms copy-pasted at three call sites; palette path silently drops Redraw
- **Severity:** medium
- **Source:** simplifier (phase 15)
- **Date:** 2026-03-16
- **File:** `arcterm-app/src/main.rs:1895-1904, 3390-3399, 3423-3428`
- **Description:** The three-arm `DispatchOutcome` match (`Redraw => request_redraw()`, `Exit => exit()`, `None => {}`) is copy-pasted at every `dispatch_action` call site. The palette call site at line 3423 omits `request_redraw()` for the `Redraw` arm, silently dropping the signal. An `execute_key_action` wrapper function exists (line 3419) but is not used at the two main call sites.
- **Remediation:** Route all call sites through `execute_key_action` (or an equivalent helper that also handles `request_redraw`), eliminating the copy-paste and the dropped-Redraw divergence.

### ISSUE-029 — menu.rs construct-then-insert pattern repeated 27 times with no helper
- **Severity:** medium
- **Source:** simplifier (phase 15)
- **Date:** 2026-03-16
- **File:** `arcterm-app/src/menu.rs:50-419`
- **Description:** Every menu item requires three lines: `MenuItem::new`, `id_map.insert(item.id().clone(), ...)`, plus a reference in the `append_items` slice. The `.id().clone()` call and `true` enabled flag appear 27 times each. A comment at line 39 acknowledges the desire for a helper but defers it.
- **Remediation:** Add a local closure `let mut item = |label, accel, action| -> MenuItem { ... }` inside `AppMenu::new()` that constructs, registers, and returns the item. Each of the 27 call sites becomes a single line.

### ISSUE-030 — SaveWorkspace date arithmetic is an inline opaque algorithm (untestable)
- **Severity:** medium
- **Source:** simplifier (phase 15)
- **Date:** 2026-03-16
- **File:** `arcterm-app/src/main.rs:1322-1343`
- **Description:** A 20-line Proleptic Gregorian date algorithm (Hinnant civil-from-days) is inlined inside a `match` arm, making it untestable and unattributed. It is the only date-formatting site in the codebase.
- **Remediation:** Extract to a `fn session_timestamp_name() -> String` function with a doc comment crediting the algorithm. Consider using `chrono` or `time` if either is already a transitive dependency.

### ISSUE-031 — `ollama_result_rx` not cleared when overlay is closed during Loading

- **Severity:** Important
- **Source:** REVIEW-2.1 (Phase 16, Plan 2.1)
- **File:** `arcterm-app/src/main.rs:3376-3379`
- **Description:** When `CmdAction::Close` is handled, `command_overlay` is set to `None` but `ollama_result_rx` is not. The spawned tokio task continues running; the drain loop in `about_to_wait` (line 2103) remains active until the task completes, holding the channel open. If the user immediately opens a new overlay and submits, the old `rx` is overwritten and dropped, the old task's send silently fails. No crash or hang results, but it is a resource leak for the duration of the LLM call.
- **Remediation:** In the `CmdAction::Close` arm, add `state.ollama_result_rx = None;` before `state.window.request_redraw();`.

### ISSUE-032 — Missing HTTP status check before deserializing Ollama generate response

- **Severity:** Important
- **Source:** REVIEW-2.1 (Phase 16, Plan 2.1)
- **File:** `arcterm-app/src/main.rs:3396-3406`
- **Description:** `client.generate()` returns `Ok(resp)` for any HTTP status including 4xx/5xx. The code immediately calls `resp.json::<GenerateChunk>()`. Ollama error bodies (`{"error":"model not found"}`) do not deserialize as `GenerateChunk`, so the user sees a misleading "parse error" instead of the actual server message.
- **Remediation:** After `Ok(resp) =>`, add: `if !resp.status().is_success() { let status = resp.status(); let _ = tx.send(Err(format!("Ollama error: {status}"))).await; return; }` before the `.json()` call.

### ISSUE-033 — `GenerateChunk.done` field is never checked; silent assumption about non-streaming response

- **Severity:** Suggestion
- **Source:** REVIEW-2.1 (Phase 16, Plan 2.1)
- **File:** `arcterm-app/src/main.rs:3398-3401` and `arcterm-app/src/ollama.rs:43-46`
- **Description:** With `stream: false`, Ollama always sets `done: true`. The code reads `chunk.response` without asserting this. The `done` field on `GenerateChunk` is unused and the assumption is undocumented.
- **Remediation:** Add `debug_assert!(chunk.done, "expected done=true for non-streaming generate response");` after deserialization, or add a comment noting the invariant.

### ISSUE-034 — No guard against submitting while a stream is in-flight

- **Severity:** Important
- **Source:** REVIEW-3.1 (Phase 16, Plan 3.1)
- **File:** `arcterm-app/src/main.rs:3803-3819`
- **Description:** Pressing Enter while `ai_state.streaming == true` unconditionally calls `add_user_message()`, replaces `ai_chat_rx`, and spawns a new tokio task. The abandoned prior task continues running, consuming Ollama resources until it naturally finishes. With a slow model and rapid user input, multiple concurrent Ollama requests can stack up silently.
- **Remediation:** Add a guard before the submission block — e.g., `if ai_state.streaming { state.window.request_redraw(); return; }` — so new messages are rejected while a response is in-flight. Optionally display a "waiting..." hint in the input bar to signal the blocked state to the user.

### ISSUE-035 — Markdown/code rendering not implemented (plan spec unmet)

- **Severity:** Important
- **Source:** REVIEW-3.1 (Phase 16, Plan 3.1)
- **File:** `arcterm-app/src/main.rs:3289-3301`
- **Description:** Plan 3.1 Task 3 Step 2 specifies "Use existing `pulldown-cmark` + `syntect` for Markdown/code in responses." The implementation renders assistant messages verbatim. Code blocks appear as raw triple-backtick text. The summary does not list this as a known deviation.
- **Remediation:** Either implement Markdown stripping before display (strip code fences, bold markers, etc.) using the already-present `pulldown-cmark` dependency, or explicitly carry this forward as a known gap in the next plan phase.

### ISSUE-036 — Streaming "..." indicator can visually overlap chat content

- **Severity:** Important
- **Source:** REVIEW-3.1 (Phase 16, Plan 3.1)
- **File:** `arcterm-app/src/main.rs:3329-3335`
- **Description:** The "  ..." streaming indicator is placed at `pane_rect.y + pane_rect.height - cell_h * 2.5`, which falls within the scrollable content area (`content_height = pane_rect.height - cell_h * 3.5`). When the chat viewport is full, the last visible chat line renders at the same y-position as the indicator, with no separating background quad to prevent visual bleed-through.
- **Remediation:** Extend the content area reservation to exclude the indicator row (e.g., `content_height = pane_rect.height - cell_h * 4.5` when streaming), or back the indicator with an `OverlayQuad` matching the pane background color at `[pane_rect.x, pane_rect.y + pane_rect.height - cell_h * 2.5, pane_rect.width, cell_h]`.

### ISSUE-037 — `finalize_response` pushes empty assistant message when stream produces no content

- **Severity:** Important
- **Source:** REVIEW-3.1 (Phase 16, Plan 3.1)
- **File:** `arcterm-app/src/ai_pane.rs:92-99`
- **Description:** `finalize_response()` unconditionally pushes `ChatMessage { role: "assistant", content: "" }` to history. If the Ollama stream completes with zero non-empty content chunks (network timeout, model error, early `break`), an empty assistant message is included in subsequent API calls, which can confuse some model implementations.
- **Remediation:** Guard the history push: `if !self.pending_response.is_empty() { self.history.push(ChatMessage { role: "assistant".to_string(), content: self.pending_response.clone() }); }`. Continue to set `streaming = false` and `pending_response.clear()` unconditionally.

### ISSUE-038 — Duplicated OllamaClient construction pattern in two spawn sites

- **Severity:** medium
- **Source:** simplifier (Phase 16)
- **File:** `arcterm-app/src/main.rs:3645-3655`, `arcterm-app/src/main.rs:3821-3829`
- **Date:** 2026-03-17T21:44:11Z
- **Description:** Two `tokio::spawn` blocks independently clone `state.config.ai.endpoint` and `state.config.ai.model`, then construct `OllamaClient::new(endpoint, model)` inside the closure with a local `use crate::ollama::OllamaClient;` import. The pattern is identical except for the subsequent API call (`generate` vs `chat`).
- **Remediation:** Add `fn ollama_client(&self) -> ollama::OllamaClient` to `AppState`. Both spawn closures call `state.ollama_client()` instead of duplicating the clone + construction. Remove the two local `use` imports.

### ISSUE-039 — `is_some().unwrap()` double-access on `ollama_result_rx`

- **Severity:** medium
- **Source:** simplifier (Phase 16)
- **File:** `arcterm-app/src/main.rs:2209-2225`
- **Date:** 2026-03-17T21:44:11Z
- **Description:** The Ollama result drain checks `if state.ollama_result_rx.is_some()` then immediately calls `.as_mut().unwrap()`. This is the anti-pattern the skill flags as AI bloat. The idiomatic Rust form is `if let Some(rx) = state.ollama_result_rx.as_mut()`.
- **Remediation:** Replace the outer guard with `if let Some(rx) = state.ollama_result_rx.as_mut() { if let Ok(result) = rx.try_recv() { ... } }`.

### ISSUE-040 — `last_ai_pane` cleanup not centralized in `remove_pane_resources`

- **Severity:** medium
- **Source:** simplifier (Phase 16)
- **File:** `arcterm-app/src/main.rs:1224-1225`, `1242-1243`, `1311-1312`, `2525-2526`
- **Date:** 2026-03-17T21:44:11Z
- **Description:** The two-line guard `if self.last_ai_pane == Some(id) { self.last_ai_pane = None; }` appears in four separate close paths instead of once inside `remove_pane_resources`. A future close path that calls `remove_pane_resources` but omits the guard will silently leave a dangling `last_ai_pane` reference.
- **Remediation:** Add the guard to `remove_pane_resources` at `arcterm-app/src/main.rs:717` and remove the four scattered copies.

### ISSUE-041 — Command overlay system prompt is an anonymous inline literal

- **Severity:** low
- **Source:** simplifier (Phase 16)
- **File:** `arcterm-app/src/main.rs:3656`
- **Date:** 2026-03-17T21:44:11Z
- **Description:** The AI pane defines its system prompt as `pub const SYSTEM_PROMPT` in `ai_pane.rs`. The command overlay's system prompt is an anonymous string literal buried inside a `tokio::spawn` closure. Both encode behavioral contracts with the model and deserve the same treatment.
- **Remediation:** Define `const GENERATE_SYSTEM_PROMPT: &str = "..."` at the top of `command_overlay.rs` and reference it at the spawn site in `main.rs`.

---

## Resolved

*(none yet)*
