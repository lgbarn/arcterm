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

---

## Resolved

*(none yet)*
