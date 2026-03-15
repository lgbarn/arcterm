# SUMMARY-3.2.md — Plan 3.2: Performance Optimization and Integration Verification

**Branch:** master
**Date:** 2026-03-15
**Commits:** 3 atomic commits on top of `2dc227b`

---

## Task 1: Performance Optimization

**Commit:** `125dd24 shipyard(phase-2): optimize for 120+ FPS with Mailbox present mode`

### Changes

- **arcterm-render/src/gpu.rs** — Changed `PresentMode` from hardcoded `Fifo` to Mailbox-with-Fifo-fallback. At init, `caps.present_modes` is checked; Mailbox is selected when supported (most modern macOS/Metal drivers), otherwise Fifo. The selected mode is logged at `debug` level.

- **arcterm-app/src/main.rs** — Added three performance fields to `AppState`:
  - `idle_cycles: u32` — counts consecutive empty PTY poll cycles
  - `fps_last_log: Instant` — timestamp of last FPS log line
  - `fps_frame_count: u32` — frames rendered since last log

  In `about_to_wait`: when PTY data arrives, `ControlFlow::Poll` is set and `idle_cycles` resets to 0. After 3 consecutive empty cycles, `ControlFlow::Wait` is set so the event loop sleeps until the next OS event (saves CPU when idle).

  In the `RedrawRequested` handler: `fps_frame_count` is incremented; every 5 s the FPS is logged at `debug` level and counters reset.

  Also in `about_to_wait`: pending DSR/DA replies are drained via `take_pending_replies()` and written back to the PTY; window title is synced from `grid.title()` to `window.set_title()`.

- **arcterm-pty/src/session.rs** — PTY reader thread buffer increased from 4096 to 16384 bytes, reducing syscall overhead for high-throughput terminal output.

- **arcterm-core/src/grid.rs** — Added `pending_replies: Vec<Vec<u8>>` field to `Grid`. Initialized as `Vec::new()` in `Grid::new()`. This is the outbound queue for DSR/DA response bytes.

- **arcterm-app/src/terminal.rs** — Added `take_pending_replies()` method which `mem::take`s `grid.pending_replies` and returns it to the caller.

### Verification

`cargo test --package arcterm-core --package arcterm-pty --package arcterm-render` — 63 tests, all passed.

---

## Task 2: App Cursor Keys + Bracketed Paste (TDD)

**Commit:** `8bb93ed shipyard(phase-2): add app cursor keys and bracketed paste support`

### TDD Note

The `translate_key_event` signature change and `translate_named` dispatch were implemented alongside the tests because winit `KeyEvent` structs cannot be constructed in a headless test environment without the full winit event loop. The testable logic was extracted into `pub(crate) fn translate_named_key` which is called directly by the unit tests. This is the minimum viable TDD approach for a GUI input module.

### Changes

- **arcterm-app/src/input.rs**:
  - `translate_key_event` signature changed from `(event, modifiers)` to `(event, modifiers, app_cursor_keys: bool)`.
  - `translate_named` now receives `app_cursor_keys` and dispatches accordingly:
    - `ArrowUp/Down/Right/Left` → `ESC O A/B/C/D` when `app_cursor_keys=true`, `ESC [ A/B/C/D` otherwise.
    - `Home` → `ESC O H` (app) / `ESC [ H` (normal).
    - `End` → `ESC O F` (app) / `ESC [ F` (normal).
    - All other keys (PageUp, PageDown, F1–F12, Enter, etc.) are mode-independent.
  - `pub(crate) fn translate_named_key` wrapper added for test access.
  - **16 unit tests** added covering all 4 arrow keys + Home/End in both normal and app cursor modes, plus 3 mode-invariant keys.

- **arcterm-app/src/main.rs** — Call site updated to read `state.terminal.grid().modes.app_cursor_keys` and pass it to `translate_key_event`.

- **Bracketed paste** — Already implemented in a prior commit (the `Cmd+V` handler in `main.rs` wraps paste text in `ESC[200~ ... ESC[201~` when `grid.modes.bracketed_paste` is true). No additional changes required.

### Verification

`cargo test --package arcterm-app` — 55 tests, all passed.

---

## Task 3: DSR/DA Responses + Integration Wiring

**Commit:** `9680381 shipyard(phase-2): add DSR/DA responses and integration polish`

### Changes

- **arcterm-vt/src/handler.rs** — `GridState` implementation:
  - `device_status_report(n: u16)`: when `n == 6`, formats `ESC[{row+1};{col+1}R` (1-indexed cursor position) and pushes to `grid.pending_replies`. All other values are no-ops.
  - `device_attributes()`: pushes `ESC[?1;2c` (VT100 with advanced video) to `grid.pending_replies`.
  - **6 unit tests** added: DSR(6) queuing with correct 1-indexed coordinates, DSR(5) no-op, DA reply format, multiple-reply accumulation, cursor visibility toggle via mode 25, app cursor key toggle via mode 1.

- **Cursor visibility check in render** — Already implemented: `build_quad_instances` in `arcterm-render/src/renderer.rs` reads `grid.modes.cursor_visible` and gates cursor quad emission behind the `is_cursor` flag. No additional changes required.

- **Window title wiring** — Wired in Task 1's `about_to_wait` addition.

- **Manual test checklist** — Documented in the module-level doc comment of `arcterm-app/src/main.rs` (present since prior phase; retained and verified).

### Verification

`cargo test --workspace` — 195 tests across 5 packages, all passed.

---

## Deviations

### Stale incremental compilation artifacts

When running `cargo test --workspace` after Task 2, the compiler reported errors in `arcterm-render` (`BG_COLOR_F32 not found`, wrong `ansi_color_to_glyphon` arity). These were caused by stale `.rmeta` files from the previous phase's build artifacts mismatching the on-disk source files (which had been updated outside of git by a prior plan execution). Running `cargo clean --package arcterm-render` resolved the issue. No source changes were needed. This is documented here as a process observation: plan phases that modify render source outside git tracking can cause stale artifact confusion on the next clean build.

### `mod colors` auto-added by linter

Between Task 1 and Task 2 commits, a linter automatically inserted `mod colors;` into `main.rs` (the `colors.rs` file already existed on disk from a prior phase). This was picked up by git as part of the `arcterm-app` build state and did not affect functionality.

---

## Final State

| Package | Tests | Result |
|---|---|---|
| arcterm-core | 51 | PASS |
| arcterm-pty | 6 | PASS |
| arcterm-render | 6 | PASS |
| arcterm-vt | 77 | PASS |
| arcterm-app | 55 | PASS |
| **Total** | **195** | **ALL PASS** |

### Performance Profile (expected at runtime)

- Present mode: **Mailbox** on Metal/Vulkan, **Fifo** fallback (logged at startup)
- ControlFlow: **Poll** when PTY active, **Wait** after 3 idle cycles
- PTY read buffer: **16384 bytes** (was 4096)
- FPS logging: every 5 s at `RUST_LOG=debug` level

### Manual Test Checklist (Task 3 acceptance criteria)

Run `cargo run --package arcterm-app` and verify:

1. `ls --color` — coloured directory listing renders correctly with ANSI colours.
2. `vim` — full-screen editor launches, redraws on resize, and exits cleanly.
3. `top` — live updating display renders without corruption.
4. `htop` — same as top; mouse input is not required for pass.
5. Window resize — drag the window edge; the shell prompt reflows to the new width.
6. Ctrl+C — sends SIGINT; running process terminates and returns to shell prompt.
7. `echo -e "\033[31mred\033[0m"` — red text appears, then colour resets.
8. Rapid output (`cat /dev/urandom | head -c 1M | base64`) — no hang or crash.
9. `tput civis` — cursor disappears; `tput cnorm` — cursor reappears.
10. OSC title: `printf '\033]0;My Title\007'` — window title bar updates.
11. DSR: `printf '\033[6n'` — cursor position report appears in output (round-trip via PTY).
12. DA: `printf '\033[c'` — device attributes response appears in output.
13. App cursor keys: enter `vim`, navigate with arrow keys — cursor moves correctly.
