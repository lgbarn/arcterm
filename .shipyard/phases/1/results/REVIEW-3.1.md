# REVIEW-3.1 — Application Shell: PTY-VT-Renderer Integration

**Plan:** 3.1
**Phase:** 1 — Foundation
**Date:** 2026-03-15
**Reviewer:** claude-sonnet-4-6
**Branch:** master

---

## Stage 1: Spec Compliance

**Verdict:** PASS (with two noted deviations; both are disclosed by the builder and one is functionally acceptable, one is a spec miss)

---

### Task 1: Terminal struct + App wiring

- Status: PASS
- Evidence:
  - `/Users/lgbarn/Personal/myterm/arcterm-app/src/terminal.rs` — `Terminal` struct owns `PtySession`, `Processor`, and `Grid`. All five required methods are present: `new`, `process_pty_output`, `write_input`, `grid`, `resize`. The `is_alive` method is extra (not in the spec) but harmless.
  - `/Users/lgbarn/Personal/myterm/arcterm-app/src/main.rs` — `main()` initializes `env_logger`, constructs a multi-thread Tokio runtime, enters it, creates `EventLoop`, and calls `event_loop.run_app`. Matches spec exactly.
  - `App` struct fields: `state: Option<AppState>` (bundles window, renderer, terminal, pty_rx), `modifiers: ModifiersState`. The plan's field layout splits these across `App` vs `AppState` differently, but the data is all present and the design is equivalent and cleaner.
  - `resumed()` creates a 1024x768 window titled "Arcterm", constructs `Renderer`, derives `GridSize`, constructs `Terminal` via `Terminal::new(size).expect(...)`. Lines 70-95.
  - `about_to_wait()` drains PTY channel with `try_recv` loop, calls `process_pty_output`, and calls `request_redraw` when data was received. Lines 98-133.
  - `window_event()` handles `RedrawRequested`, `Resized`, `CloseRequested`, `KeyboardInput`, `ModifiersChanged`. Lines 135-212.
- Notes: The plan specified `proxy.wake_up()` to signal new PTY data from a background tokio task. The builder instead drains the channel directly in `about_to_wait` (polling at the winit event cadence). This is functionally simpler and correct for Phase 1 — the plan itself described the wake_up approach as one option and said "drain it in the event loop" as the canonical description. No proxy is stored or used, which is fine.

---

### Task 2: Keyboard input + cursor rendering

- Status: PASS (with one substantive deviation: Ctrl+\\ and Ctrl+] are missing; one acceptable deviation: modifier tracking)
- Evidence:
  - `/Users/lgbarn/Personal/myterm/arcterm-app/src/input.rs` — `translate_key_event(event: &KeyEvent, modifiers: ModifiersState) -> Option<Vec<u8>>` is present with correct signature.
  - Priority 1 (text field): `Key::Character(s)` without ctrl uses `event.text` with fallback to `s.as_str()`. Lines 32-39.
  - Priority 2 (named keys): All required keys are present — Enter (`\r`), Backspace (`\x7f`), Tab (`\t`), Escape (`\x1b`), all four arrow keys, Home, End, PageUp, PageDown, Delete, F1-F12 with correct VT220 sequences. Lines 47-81.
  - Priority 3 (ctrl+alpha): `'a'..='z'` range mapped to `lower as u8 - b'a' + 1`. Ctrl+[ mapped to 0x1b. Lines 17-29.
  - Cursor rendering: `/Users/lgbarn/Personal/myterm/arcterm-render/src/text.rs` line 111-117 — inverse-video at cursor cell by swapping glyph fg to the cell's bg color via `ansi_color_to_glyphon(cell.attrs.bg, false)`. Builder correctly documents this as a partial implementation.
  - `KeyboardInput` handler calls `translate_key_event` and `write_input`, then does NOT call `request_redraw`. The spec at Task 2 explicitly says "Call `window.request_redraw()` to immediately show the echo." This call is absent from the `KeyboardInput` arm (line 192-208).
  - Modifier tracking via `ModifiersChanged` is correct for winit 0.30; builder's deviation note is accurate.
- Notes:
  1. The spec's Priority 3 also required Ctrl+\\ → 0x1c and Ctrl+] → 0x1d. Neither is implemented. The input handler returns `None` for these (the `Key::Character` ctrl arm only handles `is_ascii_alphabetic()` and `[`, silently dropping `\` and `]`). This means Ctrl+\\ (SIGQUIT) and Ctrl+] (telnet escape) do not work.
  2. `window.request_redraw()` is missing from the `KeyboardInput` handler. This means typed characters that produce no PTY echo (e.g., a shell that suppresses echo, or a full-screen app before it has had time to respond) do not immediately trigger a redraw. The spec explicitly requires this call.

---

### Task 3: Integration testing + performance measurement

- Status: PASS (with one spec miss: manual test checklist comment is absent)
- Evidence:
  - `latency-trace` feature declared in `arcterm-app/Cargo.toml` line 13. Compiles clean with and without the feature.
  - Timestamps logged at: key received (`t0` before `translate_key_event`, line 195), PTY write bytes (line 199-203), PTY output processed (line 114-118), frame submitted (line 178). All four checkpoints from the spec are covered.
  - Cold start → first frame logged via `AtomicBool::FIRST_FRAME` on the `RedrawRequested` arm, lines 181-188.
  - Shell exit (`TryRecvError::Disconnected`) logs an info message and requests one redraw without panic, lines 121-126. No `unwrap()` on the channel receiver.
  - `SurfaceError::Lost | SurfaceError::Outdated` handled in `/Users/lgbarn/Personal/myterm/arcterm-render/src/renderer.rs` line 59 — reconfigures surface and returns early.
  - PTY creation failure: uses `.expect("failed to create PTY session")` at `main.rs:88`, which panics with a message. The spec required "log the error and exit with a meaningful message (not a panic)."
  - The spec required a comment block at the top of `main.rs` documenting the manual test checklist (8 items: `ls --color`, `vim`, `top`, `htop`, window resize, Ctrl+C, echo color escape, rapid output). No such comment block exists in `main.rs`.
  - "Shell exited" display in window: the spec required displaying "Shell exited" text in the window. The implementation only logs an info message; no visual indicator is rendered to the GPU window. The `about_to_wait` handler breaks out of the loop and the window sits frozen at its last state with no text indicating the shell has exited.

---

## Stage 2: Code Quality

Stage 1 passed. Proceeding to code quality review.

---

### Critical

None.

---

### Important

- **Missing `request_redraw()` after keyboard input** — `/Users/lgbarn/Personal/myterm/arcterm-app/src/main.rs:205`
  - After `state.terminal.write_input(&bytes)`, there is no `state.window.request_redraw()` call. The spec requires it. In practice this means typed input that does not produce immediate PTY output (full-screen apps during startup, shells with echo disabled, any sub-millisecond gap between key press and PTY response) will not trigger a redraw, causing visible input lag or stutter. The fix is one line: add `state.window.request_redraw();` after `state.terminal.write_input(&bytes)` on line 205.

- **Ctrl+\\ and Ctrl+] not handled** — `/Users/lgbarn/Personal/myterm/arcterm-app/src/input.rs:22-28`
  - The ctrl branch checks `is_ascii_alphabetic()` and `== '['`, but does not handle `\\` (→ 0x1c, FS) or `]` (→ 0x1d, GS). These are specified in Priority 3 and Ctrl+\\ in particular is SIGQUIT — a commonly needed terminal control sequence. Remediation: add two more arms in the ctrl `Key::Character` branch:
    ```rust
    if lower == '\\' { return Some(vec![0x1c]); }
    if lower == ']'  { return Some(vec![0x1d]); }
    ```

- **PTY creation failure uses `expect` (panics) instead of a graceful exit** — `/Users/lgbarn/Personal/myterm/arcterm-app/src/main.rs:88`
  - `Terminal::new(size).expect("failed to create PTY session")` panics with a Rust backtrace. The spec requires logging the error and exiting with a meaningful message. Remediation: use `.unwrap_or_else(|e| { log::error!("Failed to create PTY session: {e}"); std::process::exit(1); })` or propagate the error through `resumed` and handle it with `event_loop.exit()`.

- **Shell exit produces no visible "Shell exited" indicator** — `/Users/lgbarn/Personal/myterm/arcterm-app/src/main.rs:121-126`
  - The spec required that when the shell exits, "Shell exited" text is displayed in the window so the user understands the window is inert. The current implementation only logs to stderr. The frozen window is indistinguishable from a crashed or hung process. Remediation: set a flag on `AppState` (e.g., `shell_exited: bool`) and in `RedrawRequested` write a "Shell exited — press any key to close" message into the grid or render it as an overlay before calling `render_frame`.

---

### Suggestions

- **Manual test checklist comment absent from `main.rs`** — `/Users/lgbarn/Personal/myterm/arcterm-app/src/main.rs` (top of file)
  - The spec required an 8-item manual test checklist as a comment block at the top of `main.rs`. This is documentation for the developer running manual verification, and serves as the acceptance criteria record. Remediation: add the checklist as a `//!` block at the top of `main.rs` covering all 8 items from Task 3.

- **`is_alive()` is dead code** — `/Users/lgbarn/Personal/myterm/arcterm-app/src/terminal.rs:64`
  - The compiler emits a `dead_code` warning for `is_alive`. The summary acknowledges this. If the shell-exit behavior is remediated by tracking `TryRecvError::Disconnected` on the channel (which is already done), this method serves no purpose in the current design. Either use it in the shell-exit display path or mark it `#[allow(dead_code)]` with a comment explaining it is reserved for future use, or remove it.

- **Cursor inverse-video only changes glyph color, not cell background** — `/Users/lgbarn/Personal/myterm/arcterm-render/src/text.rs:112-117`
  - As documented by the builder, the cursor is shown only by recoloring the text glyph at the cursor cell to the background palette color. On a cell that holds a space character (very common — the cursor is often parked on an empty cell), the glyph renders nothing and the cursor is invisible. The builder acknowledges a full block cursor needs a wgpu rectangle pass; that is out of Phase 1 scope. However, this means the cursor can be fully invisible on blank cells, which is a UX regression from a typical terminal. This should be tracked for Phase 2.

- **`about_to_wait` continues polling after shell exits** — `/Users/lgbarn/Personal/myterm/arcterm-app/src/main.rs:98-133`
  - After `TryRecvError::Disconnected`, the code breaks out of the inner loop, but `about_to_wait` is called on every event loop tick forever. There is no mechanism to stop polling. The receiver will return `Disconnected` on every subsequent call. This is harmless (it is cheap), but a `shell_exited: bool` flag on `AppState` would let `about_to_wait` skip the loop entirely after the first disconnect, which is cleaner.

- **`scale_factor` cast from `f64` to `f32` silently** — `/Users/lgbarn/Personal/myterm/arcterm-app/src/main.rs:85` and `163`
  - `window.scale_factor()` returns `f64`. It is passed to `grid_size_for_window` and `render_frame`. If these take `f32`, the cast is implicit via `as f32` somewhere in the call chain. Explicit casting with a comment (`as f32 // HiDPI scale, precision loss is acceptable`) would make the narrowing intentional and visible.

---

## Issues Appended to ISSUES.md

The following non-blocking items are appended to `/Users/lgbarn/Personal/myterm/.shipyard/ISSUES.md` per the Issue Tracking Protocol.

- **ISSUE-002** — Missing `request_redraw()` after keyboard input (Important, REVIEW-3.1, `arcterm-app/src/main.rs:205`)
- **ISSUE-003** — Ctrl+\\ and Ctrl+] not handled (Important, REVIEW-3.1, `arcterm-app/src/input.rs:22-28`)
- **ISSUE-004** — PTY creation failure panics instead of graceful exit (Important, REVIEW-3.1, `arcterm-app/src/main.rs:88`)
- **ISSUE-005** — Shell exit produces no visible indicator in window (Important, REVIEW-3.1, `arcterm-app/src/main.rs:121-126`)
- **ISSUE-006** — Cursor invisible on blank cells due to text-only inverse-video approach (Suggestion, REVIEW-3.1, `arcterm-render/src/text.rs:112-117`)

---

## Summary

**Verdict:** REQUEST CHANGES

The core wiring (PTY-VT-Grid-Renderer loop, keyboard input, latency tracing, build) is correct and both verify commands pass. However, four Important spec requirements from Task 2 and Task 3 are unimplemented: the `request_redraw()` call after keyboard input is missing (causing input lag), Ctrl+\\ and Ctrl+] are silently dropped, PTY creation failures panic rather than exit gracefully, and the "Shell exited" in-window display specified in Task 3 error handling is not present. The cursor is also invisible on blank cells, which is the most common cursor position, making it functionally invisible in practice. These issues do not prevent compilation or basic use, but they represent directly specified behavior that was not built.

Critical: 0 | Important: 4 | Suggestions: 4
