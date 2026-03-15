---
phase: foundation
plan: "2.2"
wave: 2
dependencies: ["1.1"]
must_haves:
  - PtySession spawns a shell and reads output via mpsc channel
  - PtySession accepts input writes
  - PtySession supports resize
  - Integration test confirms shell I/O round-trip
files_touched:
  - arcterm-pty/Cargo.toml
  - arcterm-pty/src/lib.rs
  - arcterm-pty/src/session.rs
tdd: true
---

# Plan 2.2 -- PTY Session Management

**Wave 2** | Depends on: Plan 1.1 (arcterm-core types) | Parallel with: Plans 2.1, 2.3

## Goal

Implement `PtySession` that spawns a shell process attached to a PTY, reads output on a background thread via an mpsc channel, and accepts keyboard input writes. After this plan, a test can spawn a shell, send `echo hello`, and receive the output.

---

<task id="1" files="arcterm-pty/src/session.rs, arcterm-pty/src/lib.rs" tdd="true">
  <action>
    Implement `PtySession` in `arcterm-pty/src/session.rs`.

    **Structure:**
    ```rust
    pub struct PtySession {
        master: Box<dyn portable_pty::MasterPty + Send>,
        writer: Box<dyn std::io::Write + Send>,
        output_rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
        child: Box<dyn portable_pty::Child + Send + Sync>,
    }
    ```

    **`PtySession::new(size: GridSize) -> Result<Self, PtyError>`:**
    1. Call `portable_pty::native_pty_system()`.
    2. Open PTY pair with `PtySize { rows: size.rows as u16, cols: size.cols as u16, pixel_width: 0, pixel_height: 0 }`.
    3. Build command: detect user's shell from `$SHELL` env var, fall back to `/bin/bash` on Unix, `cmd.exe` on Windows.
    4. Set `TERM=xterm-256color` environment variable on the command.
    5. Spawn command on slave, then drop slave.
    6. Take writer from master.
    7. Clone reader from master via `try_clone_reader()`.
    8. Create `tokio::sync::mpsc::channel::<Vec<u8>>(64)`.
    9. Spawn a `std::thread::spawn` (not tokio spawn_blocking, since we need it to live for the session lifetime) that loops: read 4096-byte buffer from reader, `tx.blocking_send(buf[..n].to_vec())`, break on read 0 or error.
    10. Return `PtySession { master, writer, output_rx, child }`.

    **Methods:**
    - `pub fn write(&mut self, data: &[u8]) -> std::io::Result<()>` -- writes to PTY writer.
    - `pub fn try_recv(&mut self) -> Option<Vec<u8>>` -- non-blocking receive from output channel. Use `output_rx.try_recv().ok()`.
    - `pub async fn recv(&mut self) -> Option<Vec<u8>>` -- async receive from output channel.
    - `pub fn resize(&self, size: GridSize) -> Result<(), PtyError>` -- calls `master.resize()`.
    - `pub fn is_alive(&mut self) -> bool` -- calls `child.try_wait()`, returns true if still running.

    **Error type:**
    `PtyError` enum with variants: `SpawnFailed(String)`, `IoError(std::io::Error)`, `ResizeFailed(String)`.
    Implement `From<std::io::Error>` for `PtyError`.

    **`lib.rs`:** Re-export `PtySession` and `PtyError`.

    **Tests (write first):**
    - `test_spawn_shell`: Create PtySession with 80x24. Verify `is_alive()` returns true. Drop session (child should be cleaned up).
    - `test_write_and_read`: Spawn shell. Write `b"echo hello_pty_test\n"`. Receive output within 2 seconds (use tokio::time::timeout). Assert received bytes contain "hello_pty_test".
    - `test_resize`: Spawn shell. Resize to 120x40. Verify no error. (Cannot easily verify the kernel accepted it, but the call should not panic or error.)
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-pty 2>&1 | tail -15</verify>
  <done>All PtySession tests pass. Shell spawns successfully, I/O round-trip works (echo command output received), resize completes without error. Tests use `#[tokio::test]` for async recv.</done>
</task>

<task id="2" files="arcterm-pty/src/session.rs" tdd="true">
  <action>
    Add robustness features and additional tests:

    **Reader thread naming:** Name the reader thread "pty-reader" via `std::thread::Builder::new().name("pty-reader".into()).spawn(...)` for debuggability.

    **Graceful shutdown:** Add `pub fn shutdown(&mut self)` that:
    1. Drops the writer (signals EOF to the shell).
    2. Waits for child to exit with a timeout (use `child.wait()` -- note this blocks, so document that shutdown should be called from a context where blocking is acceptable, or wrap in spawn_blocking in the app layer).
    3. The reader thread will exit naturally when the PTY master closes.

    **Tests:**
    - `test_shell_exit_detection`: Spawn shell. Write `b"exit\n"`. Poll `is_alive()` in a loop with timeout. Assert it eventually returns false.
    - `test_recv_after_exit`: Spawn shell. Write `b"echo goodbye && exit\n"`. Collect all output via `recv()` until it returns `None`. Assert collected output contains "goodbye".
    - `test_write_after_exit`: Spawn shell. Write `b"exit\n"`. Wait for child to exit. Write `b"this should fail"`. Assert the write returns an error (broken pipe).
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-pty 2>&1 | tail -15</verify>
  <done>All robustness tests pass. Reader thread is named "pty-reader". Shell exit is detected. Output is fully drained before channel closes. Write after exit returns an error. Total test count for arcterm-pty is at least 6.</done>
</task>
