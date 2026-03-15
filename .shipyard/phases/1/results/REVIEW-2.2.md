# REVIEW-2.2 — PTY Session Management

**Reviewer:** Claude Code (Senior Review Agent)
**Date:** 2026-03-15
**Plan:** PLAN-2.2
**Verdict:** FAIL — SPEC_DEVIATIONS

---

## Stage 1: Spec Compliance

**Verdict:** FAIL

### Task 1: PtySession Core

- Status: FAIL
- Evidence: `/Users/lgbarn/Personal/myterm/arcterm-pty/src/session.rs`

**Deviation 1 — `output_rx` omitted from struct; constructor signature changed.**

The plan specifies the struct as:

```rust
pub struct PtySession {
    master: Box<dyn portable_pty::MasterPty + Send>,
    writer: Box<dyn std::io::Write + Send>,
    output_rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}
```

and the constructor signature as:

```rust
pub fn new(size: GridSize) -> Result<Self, PtyError>
```

The implementation drops `output_rx` from the struct and changes the constructor to return `Result<(Self, mpsc::Receiver<Vec<u8>>), PtyError>`. The `PtySession` struct at `session.rs:35-39` has only three fields (`master`, `writer`, `child`).

This is not a minor naming deviation. The spec's design places the receiver inside the session so that `try_recv` and `recv` can be called on the session itself. The implementation externalises the receiver, which changes the API contract visible to all downstream consumers (`arcterm-app` and future crate integrations).

**Deviation 2 — `try_recv` and `recv` methods not implemented.**

The plan specifies two methods on `PtySession`:

- `pub fn try_recv(&mut self) -> Option<Vec<u8>>` — non-blocking receive via `output_rx.try_recv().ok()`
- `pub async fn recv(&mut self) -> Option<Vec<u8>>` — async receive from output channel

Neither method exists in `session.rs`. Searching the file confirms no function named `try_recv` or `recv` is present. The tests access the receiver directly as a variable returned from `new()`, which circumvents the missing methods and masks their absence.

**What is correctly implemented in Task 1:**

- `NativePtySystem::default()` used (equivalent to `native_pty_system()`).
- `PtySize` constructed with correct field mapping from `GridSize`.
- Shell detection from `$SHELL`, fallback to `/bin/bash` (Unix) / `cmd.exe` (Windows) at `session.rs:62-68`.
- `TERM=xterm-256color` set via `cmd.env` at `session.rs:71`.
- Slave dropped after spawn at `session.rs:79`.
- `take_writer()` and `try_clone_reader()` both called.
- `mpsc::channel::<Vec<u8>>(64)` created.
- Read loop uses 4096-byte buffer, breaks on `Ok(0)` or error, sends via `blocking_send`.
- `PtyError` enum with `SpawnFailed`, `IoError`, `ResizeFailed` at `session.rs:10-14`.
- `From<io::Error>` implemented at `session.rs:16-20`.
- `write`, `resize`, `is_alive` all present and correct.
- Three Task 1 tests (`test_spawn_shell`, `test_write_and_read`, `test_resize`) exist and pass.
- `lib.rs` re-exports `PtySession` and `PtyError` at `lib.rs:5`.

### Task 2: Robustness Features

- Status: FAIL
- Evidence: `/Users/lgbarn/Personal/myterm/arcterm-pty/src/session.rs`

**Deviation 3 — `test_write_after_exit` not implemented.**

The plan specifies three tests for Task 2:

- `test_shell_exit_detection` — present at `session.rs:219`.
- `test_recv_after_exit` — present at `session.rs:240`.
- `test_write_after_exit` — **absent**. No function with this name or equivalent exists in the test module. `grep` finds no reference to "write_after_exit" or "broken pipe" in `session.rs`.

The plan's done criteria for Task 2 states explicitly: "Write after exit returns an error. Total test count for arcterm-pty is at least 6." The test suite contains 5 tests, confirmed by `cargo test --package arcterm-pty` output: `5 passed`. The done criterion is not met.

**Deviation 4 — `shutdown()` does not reliably close the write end.**

The plan specifies: "Drops the writer (signals EOF to the shell)." The implementation replaces `self.writer` with `Box::new(io::sink())` at `session.rs:153`. `io::sink()` discards all written bytes; it does not close any file descriptor. The original writer (`Box<dyn Write + Send>`) is dropped by `std::mem::replace`, but whether that drop actually closes the underlying PTY write end depends on `portable_pty`'s implementation. The SUMMARY describes this as "closes the write end," but `io::sink()` is a no-op writer that opens no resource and closes nothing. The actual effect is the drop of the original `Box<dyn Write>` — the `io::sink()` replacement is an indirect mechanism that works only because of the drop, not because the sink signals EOF. The comment in the code is misleading and the mechanism is fragile: if `portable_pty` internally clones the write handle, replacing one `Box` with a sink would not close the PTY write end at all.

The plan's intent is clear: drop the writer to trigger EOF. The correct implementation is `drop(std::mem::replace(&mut self.writer, Box::new(io::sink())))` — which is what actually happens — but the mechanism relies on an implementation detail of the boxed writer. More robustly, the writer field type should be `Option<Box<dyn Write + Send>>` so `shutdown` can `take()` it and the drop is unambiguous. This is an Important finding, not a Critical blocker, but it compounds the existing deviations.

**What is correctly implemented in Task 2:**

- Reader thread named `"pty-reader"` via `std::thread::Builder::new().name("pty-reader".to_string())` at `session.rs:95`.
- `shutdown()` method present at `session.rs:150-155`.
- `child.wait()` called inside `shutdown()`.
- `test_shell_exit_detection` polls `is_alive()` with 50 ms sleeps and a 5-second timeout.
- `test_recv_after_exit` drains the channel and asserts it contains "goodbye".

---

## Stage 2: Code Quality

Stage 2 is withheld pending Stage 1 remediation. Two deviations (missing `try_recv`/`recv` methods, missing `test_write_after_exit`) must be fixed first.

---

## Issues Logged

The following non-blocking findings are appended to `.shipyard/ISSUES.md` per protocol.

**Important — `shutdown()` writer-drop mechanism is implicit and fragile** (`session.rs:153`).
Replacing the writer with `io::sink()` achieves EOF only because the original `Box<dyn Write>` is dropped by `std::mem::replace`. If `portable_pty` internally clones the write handle (some backends do), this does not actually close the write end. Remediation: Change `writer` field to `Option<Box<dyn Write + Send>>`. In `shutdown`, call `self.writer.take()` and let the returned `Box` drop explicitly. In `write`, return `Err(io::Error::new(io::ErrorKind::BrokenPipe, "session shut down"))` if the option is `None`.

---

## Required Fixes Before Re-Review

1. **Add `output_rx: tokio::sync::mpsc::Receiver<Vec<u8>>` to the `PtySession` struct** and revert `new()` to return `Result<Self, PtyError>`. Move the receiver into the struct.
2. **Implement `pub fn try_recv(&mut self) -> Option<Vec<u8>>`** on `PtySession` using `self.output_rx.try_recv().ok()`.
3. **Implement `pub async fn recv(&mut self) -> Option<Vec<u8>>`** on `PtySession` using `self.output_rx.recv().await`.
4. **Update tests** to call `session.try_recv()` / `session.recv()` rather than holding a separate `rx` variable.
5. **Add `test_write_after_exit`**: spawn shell, write `b"exit\n"`, poll until `is_alive()` is false, then call `session.write(b"this should fail")` and assert the result is `Err` (broken pipe or similar I/O error).

---

## Summary

**Verdict:** BLOCK

The implementation deviates from the spec on two structural points: the `PtySession` struct is missing the `output_rx` field and the corresponding `try_recv`/`recv` methods, and the `test_write_after_exit` test required by Task 2's done criteria is absent. All existing tests pass, and the core mechanics (shell spawning, I/O, reader thread naming, resize, exit detection) are correctly implemented. The fixes are well-scoped and do not require redesigning the implementation.

**Critical:** 0 | **Important:** 3 (2 spec deviations + 1 shutdown robustness) | **Suggestions:** 0
