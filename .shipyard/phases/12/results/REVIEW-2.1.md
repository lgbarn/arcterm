# Review: Plan 2.1

## Verdict: CRITICAL_ISSUES

---

## Findings

### Critical

---

**CRITICAL-1: `Pty` is dropped at end of `Terminal::new()`, sending SIGHUP to the shell immediately after spawn**

- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/terminal.rs:254–445`
- **Description:** `let pty = tty::new(...)` creates the PTY at line 254. Two file handles are cloned from it (`pty_file_for_write` at line 291, `pty_file_for_read` at line 331). The `pty` local variable is never stored in the `Terminal` struct. The comment block at lines 414–423 explicitly acknowledges this, asserting "the child process will be SIGCHLD'd when the fd closes." This reasoning is wrong.

  `alacritty_terminal::tty::unix::Pty` has an explicit `Drop` implementation (verified at line 306–318 of `alacritty_terminal-0.25.1/src/tty/unix.rs`):
  ```rust
  impl Drop for Pty {
      fn drop(&mut self) {
          unsafe { libc::kill(self.child.id() as i32, libc::SIGHUP); }
          unregister_signal(self.sig_id);
          let _ = self.child.wait();
      }
  }
  ```
  This sends `SIGHUP` to the child PID and reaps the zombie. Since `pty` falls out of scope at the end of `Terminal::new()` (around line 445), every newly spawned terminal receives SIGHUP during construction. The reader/writer threads get valid file descriptor clones but the shell process has already been killed. The terminal appears to initialize correctly, the OS fd handles remain open, but no shell is running behind them.

  This is the root cause of any "terminal opens but immediately appears dead" behavior. The tests pass because none of them spawn an actual PTY.

- **Remediation:** Store the `Pty` in `Terminal`. The concern about "Drop-ordering issues with reader/writer threads" in the comment is addressable: the writer thread owns a cloned `File` (not the `Pty`), and the reader thread owns a cloned `File`. Neither thread holds a reference to `Pty` itself. Dropping `Pty` last (when `Terminal` drops) is safe and correct.

  Add a field to `Terminal`:
  ```rust
  /// Owns the PTY lifecycle: keeps the child alive and handles cleanup on drop.
  _pty: alacritty_terminal::tty::Pty,
  ```
  Populate it in the struct literal:
  ```rust
  Terminal {
      _pty: pty,
      // ... other fields
  }
  ```
  The `Pty`'s `Drop` will then run after the reader/writer threads have their channels closed (when `Terminal` drops its `wakeup_rx`, `apc_rx`, etc.), which is the correct shutdown order: channels close → threads exit their loops → Pty drops → SIGHUP + wait.

  Note: the writer thread does not hold the `Pty`; it holds a `File` clone. The reader thread similarly holds a `File` clone. Neither is borrowed from `Pty`, so storing `Pty` in `Terminal` introduces no borrow conflict.

---

**CRITICAL-2: `tiocswinsz` sends only `SIGWINCH`; `TIOCSWINSZ` ioctl is never called**

- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/terminal.rs:502–514`
- **Description:** The `tiocswinsz` function's comment claims it will call `TIOCSWINSZ` but the actual code only calls `libc::kill(child_pid, SIGWINCH)`. The `TIOCSWINSZ` and `ioctl` imports are suppressed with `let _ = (window_size, TIOCSWINSZ, ioctl)`. `SIGWINCH` alone does not update the kernel's terminal window size record — it signals the shell that a resize occurred, but `ioctl(TIOCGWINSZ)` on the PTY master fd will still return the original size. Programs that query the terminal size (e.g., `stty size`, ncurses, `COLUMNS`/`LINES` via `ioctl`) will see the old dimensions. This means every resize operation leaves the PTY at the original size.

  The root cause is that the PTY master fd was moved into the writer thread and is not accessible for an ioctl call from the main thread. The plan spec (Task 2, acceptance criterion) requires: "Resize propagates to both Term and PTY."

- **Remediation:** Store the raw PTY master fd number (as a `std::os::unix::io::RawFd`, obtained before moving the `File` into the writer thread) in `Terminal` as a field. In `tiocswinsz`, call `ioctl(raw_fd, TIOCSWINSZ, &winsize)` directly using the stored fd number. The fd remains valid as long as the writer thread is alive (its `File` clone keeps it open). Alternatively, pass a `WindowSize` via a dedicated resize channel to the writer thread, which can call `ioctl` on its own `File` handle.

  ```rust
  // In Terminal::new(), before moving pty_file_for_write:
  let pty_master_fd: std::os::unix::io::RawFd = pty_file_for_write.as_raw_fd();
  // Store in Terminal:
  pty_master_fd: RawFd,
  ```
  ```rust
  fn tiocswinsz(&self, window_size: WindowSize) {
      #[cfg(unix)]
      unsafe {
          let winsize = libc::winsize {
              ws_row: window_size.num_lines,
              ws_col: window_size.num_cols,
              ws_xpixel: window_size.cell_width * window_size.num_cols,
              ws_ypixel: window_size.cell_height * window_size.num_lines,
          };
          libc::ioctl(self.pty_master_fd, libc::TIOCSWINSZ, &winsize);
      }
  }
  ```

---

### Minor

---

**MINOR-1: `dispatch_osc7770` uses a `thread_local!` accumulator — state leaks across panes if called from the same thread**

- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/terminal.rs:856–911`
- **Description:** `dispatch_osc7770` stores the in-progress accumulator in a `thread_local! { static ACTIVE: RefCell<Option<...>> }`. This function is called from the main thread (via `has_wakeup` → draining `osc7770_rx`). If two panes both have OSC 7770 streams and their events are interleaved in the drain (e.g., pane A opens a block, pane B closes a block, pane A closes its block), the ACTIVE state from pane A will incorrectly be used when processing pane B's `"end"` event. The architecture drains all panes sequentially on the same thread, so this is a real multi-pane interleaving risk.

- **Remediation:** Move the accumulator state out of `thread_local!` and into the per-`Terminal` struct directly. Add a field `active_osc7770: Option<StructuredContentAccumulator>` to `Terminal`, and change `dispatch_osc7770` to take `active: &mut Option<StructuredContentAccumulator>` as a parameter instead of using `ACTIVE`.

---

**MINOR-2: `strip_ansi` corrupts multi-byte UTF-8 characters**

- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/terminal.rs:917–965`
- **Description:** The function processes `s.as_bytes()` one byte at a time and pushes each non-control byte as `bytes[i] as char`. For UTF-8 continuation bytes (0x80–0xBF), this produces individual replacement-range characters instead of the original Unicode code point. The input `s: &str` is already valid UTF-8 (it came from `String::from_utf8` on the captured bytes), so multi-byte sequences like `é` (0xC3 0xA9) produce two junk `char` values (`Ã` and `©`) in the output.

- **Remediation:** Iterate over `char` values directly rather than bytes:
  ```rust
  fn strip_ansi(s: &str) -> String {
      // Work on bytes for escape detection but decode chars for output.
      // Simpler: use a char-based state machine.
      let mut result = String::with_capacity(s.len());
      let bytes = s.as_bytes();
      let mut i = 0;
      while i < bytes.len() {
          if bytes[i] == 0x1b { /* ... ESC handling unchanged ... */ }
          else if bytes[i] < 0x20 && bytes[i] != b'\n' && bytes[i] != b'\t' {
              i += 1;
          } else {
              // Decode the full UTF-8 character at this position.
              let rest = std::str::from_utf8(&bytes[i..]).unwrap_or("");
              if let Some(ch) = rest.chars().next() {
                  result.push(ch);
                  i += ch.len_utf8();
              } else {
                  i += 1; // skip invalid byte
              }
          }
      }
      result
  }
  ```

---

**MINOR-3: `splitn(3, ';')` in `dispatch_osc7770` drops OSC 7770 attributes beyond the third field**

- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/terminal.rs:863`
- **Description:** `params.splitn(3, ';').collect()` limits the split to 3 parts. For a params string like `"start;type=code;lang=rust;id=42"`, the result is `["start", "type=code", "lang=rust;id=42"]`. The third element `"lang=rust;id=42"` is treated as a single key=value pair where `v = "rust;id=42"` — the `id` attribute is silently lost.

- **Remediation:** Change `splitn(3, ';')` to `split(';')` and collect into a `Vec<&str>`. The `parts.first()` discriminator check becomes a separate step:
  ```rust
  let mut parts = params.splitn(2, ';');
  let verb = parts.next().unwrap_or("");
  let rest = parts.next().unwrap_or("");
  // Parse rest with split(';') for attributes.
  ```

---

**MINOR-4: `write_input` silently drops data when the write channel is full**

- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/terminal.rs:459`
- **Description:** `self.write_tx.try_send(bytes)` returns `Err(TrySendError::Full)` when all 16 channel slots are occupied. The error is logged at `warn` level and the data is discarded. For interactive keyboard input this produces silent dropped keystrokes with no feedback to the user or caller.

- **Remediation:** For interactive input paths that must not drop data, use `send` (blocking) instead of `try_send`, or add backpressure signaling. A practical approach: keep `try_send` for high-frequency paths like terminal resize replies, but add a separate `write_input_blocking` path (using `send`) for user keyboard input called from the main thread, where blocking briefly is acceptable.

---

**MINOR-5: `has_wakeup` returns `false` for a cleanly exited terminal whose reader thread broke from its loop without sending a wakeup**

- **File:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/terminal.rs:522–546`, `main.rs:1444–1458`
- **Description:** The reader thread calls `break` on PTY EOF (line 353) without sending a final wakeup signal. If the shell exits cleanly (EOF, no `ChildExit` event), `wakeup_rx` may be empty when `has_wakeup` is called, causing `has_wakeup` to return `false`. The `has_exited` check in `main.rs:1452` guards closed-pane detection separately, but the drain logic at line 1458 only runs under `had_wakeup || has_exited`, so `has_exited` alone does gate drain. However, `ArcTermEventListener::send_event(ChildExit)` does call `wakeup_tx.send(())` (line 127), so a `ChildExit` event from alacritty will trigger a wakeup. The edge case is a clean reader-EOF without a `ChildExit` event — in that scenario `has_exited` returns false (exit_code is None), the pane stays open, and the drain loop is missed. This is a low-severity edge case in practice but worth noting.

- **Remediation:** In the reader thread's EOF handler, before `break`, send a final wakeup:
  ```rust
  Ok(0) => {
      log::debug!("PTY reader: EOF");
      let _ = wakeup_tx_clone.send(()); // wake main thread for final drain
      break;
  }
  ```
  This requires cloning `wakeup_tx` into the reader thread closure.

---

### Positive

- **Architecture diagram in module doc** (`terminal.rs:1–23`) is accurate and matches the implementation exactly. Reader → PreFilter → parser → Term, with the three side channels shown. Clear and useful.

- **Direct parser approach** is the correct implementation choice given `EventLoop`'s ownership model. The plan explicitly approved this fallback; the builder chose it as primary and documented why. The result is simpler than the pipe approach would have been.

- **Child PID extraction** at line 255 (`pty.child().id()`) correctly happens before any file cloning, satisfying the plan's explicit requirement.

- **ArcTermSize** custom struct at lines 57–84 is the right call. Avoids test-module coupling and has full documentation.

- **`to_arcterm_grid()` bridge** at lines 717–760 is well-scoped: named clearly as a bridge, doc comment explicitly states it is removed in Plan 3.1, and it avoids holding the FairMutex lock longer than necessary by returning an owned value.

- **Wakeup side-channel consolidation** — the deviation of draining all four channels in `has_wakeup()` rather than providing separate drain calls is an improvement over the plan. It reduces main-loop complexity and is correctly documented in the SUMMARY.

- **Writer thread partial-write loop** (lines 306–321) handles `WouldBlock` and `Interrupted` correctly. Most implementations omit the partial-write loop entirely.

- **Test coverage**: 322 tests pass, verified with `cargo test -p arcterm-app`. The two module-level tests (`async_image_decode_via_channel`, `osc7770_dispatch_start_end`) are meaningful behavior tests, not implementation-detail tests.

- **All 4 `Terminal::new()` call sites updated** in `main.rs` (lines 349, 848, 928, 1085) with the correct new signature. `pty_channels` references removed throughout and replaced with accurate comments.

---

## Stage 1: Spec Compliance

### Task 1: EventListener and Terminal struct
- **Status:** PASS with critical defect
- **Evidence:** `ArcTermEventListener` at lines 99–149 correctly implements all four required event routes. `Terminal` struct at lines 182–220 contains all required fields. `Terminal::new()` at lines 230–446 executes the construction sequence correctly through steps 1–8.
- **Notes:** The struct is complete and correct. The critical defect (CRITICAL-1) is that `pty` is dropped at end of `new()` rather than stored in `Terminal`, which kills the child process immediately. This is a runtime failure, not a compilation failure, which is why tests pass.

### Task 2: Rewire AppState
- **Status:** PASS
- **Evidence:** `pty_channels` HashMap removed from `AppState` (grep confirms no occurrences; comments at lines 2923, 2949, 3029, 3242 note removal). Four `Terminal::new()` call sites updated (lines 349, 848, 928, 1085). `has_wakeup()` used at line 1446. `resize()` with four parameters at lines 1748, 2896, 3209. `to_arcterm_grid()` used at lines 2068, 2121, 2398. All drain methods wired in the `about_to_wait` loop.
- **Notes:** Fully compliant with Task 2 spec. The `cell_dims()` helper addition is a clean solution to the pre-renderer-init resize problem.

### Task 3: Pre-filter output pipeline
- **Status:** PASS
- **Evidence:** OSC 7770 capture in reader thread (lines 347, 379–393). `dispatch_osc7770` with captured text (lines 851–911). `strip_ansi` cleanup (lines 917–965). APC/Kitty wired via `process_kitty_payload` (lines 806–837). OSC 133 drain in `has_wakeup` (lines 541–543) → `take_exit_codes` (lines 587–599). Test `osc7770_dispatch_start_end` at line 1110 verifies non-empty buffer — addresses REVIEW-1.1-A.
- **Notes:** Task 3 is fully implemented. MINOR-1 (thread-local accumulator leaks across panes) and MINOR-2 (UTF-8 corruption in strip_ansi) affect correctness but are edge cases that will not manifest in single-pane or ASCII-only usage.

---

## Summary

The implementation is structurally complete and matches the plan's intent across all three tasks. All 322 tests pass. However, **CRITICAL-1** — the `Pty` being dropped at the end of `Terminal::new()` — means every spawned terminal immediately receives `SIGHUP` and the shell is killed during construction. The fix is mechanical: add `_pty: alacritty_terminal::tty::Pty` to the `Terminal` struct. **CRITICAL-2** (missing `TIOCSWINSZ` ioctl on resize) means every resize operation fails to update the kernel terminal size. Both criticals must be resolved before this branch is functional as a terminal.
