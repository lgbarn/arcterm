---
plan: 01
phase: event-handling-exit-hardening
status: complete
commits:
  - 6b9a915
  - 0aa378a
  - 1c01e76
---

# SUMMARY-01 — EOF Wakeup, Auto-Close Exit, Fifo Logging

## Tasks Completed

### Task 1 — Reader thread EOF wakeup (arcterm-app/src/terminal.rs)

**What was done:**
- Added `let wakeup_tx_for_reader = wakeup_tx.clone();` immediately after
  the channel is created at line 284, before `wakeup_tx` is moved into
  `ArcTermEventListener` at line 289.
- Moved `wakeup_tx_for_reader` into the PTY reader thread closure via capture.
- Added `let _ = wakeup_tx_for_reader.send(());` before `break` in the `Ok(0)`
  (EOF) arm.
- Added the same send before `break` in the `Err(e)` arm for symmetry.

**Verification:** `cargo build -p arcterm-app` — clean build, no warnings.

### Task 2 — Remove exit banner, auto-close immediately (arcterm-app/src/main.rs)

**What was done:**
- In `about_to_wait`, replaced `state.shell_exited = true; state.window.request_redraw();`
  with `event_loop.exit(); return;` in the `state.panes.is_empty()` block.
- Removed the entire 36-line banner overlay render block (`if state.shell_exited { ... }`)
  from the `RedrawRequested` handler.
- Removed the 6-line "press any key to close" keyboard handler block from
  `WindowEvent::KeyboardInput`.
- Removed `shell_exited: bool` field from the `AppState` struct.
- Removed `shell_exited: false` from the struct initialization literal.
- Removed `self.shell_exited = false` from the workspace restore function.
- Removed the early-return guard `if state.shell_exited { event_loop.exit(); return; }`
  from the top of `about_to_wait`.

**Verification:** `cargo build -p arcterm-app` succeeded and
`grep -c "shell_exited" arcterm-app/src/main.rs` returned `0`.

**Net change:** -59 lines, +4 lines.

### Task 3 — Fifo fallback logging (arcterm-render/src/gpu.rs)

**What was done:**
- Added `let fifo_available = caps.present_modes.contains(&wgpu::PresentMode::Fifo);`
  after capability query.
- Added `log::warn!(...)` if `!fifo_available`, listing the actual supported modes.
- Changed `log::debug!("wgpu present mode: {:?}", present_mode)` to
  `log::info!("wgpu present mode: {:?} (fifo supported: {})", present_mode, fifo_available)`.

**Verification:** `cargo build -p arcterm-render` — clean build.

## Final Verification

- `cargo test --workspace`: 41 passed, 0 failed.
- `cargo clippy --workspace -- -D warnings`: no warnings, clean.

## Deviations

None. All changes implemented exactly as specified in the plan.
