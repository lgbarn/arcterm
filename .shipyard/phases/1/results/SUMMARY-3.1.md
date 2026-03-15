# SUMMARY-3.1 — Application Shell: PTY-VT-Renderer Integration

**Plan:** 3.1
**Phase:** 1 — Foundation
**Date:** 2026-03-15
**Branch:** master
**Final status:** All 3 tasks complete, all verify commands pass.

---

## Tasks Completed

### Task 1 — Terminal struct + App wiring

**New files:**
- `arcterm-app/src/terminal.rs` — `Terminal` struct wrapping `PtySession`, `Processor`, `Grid`. Public API: `new(size) -> Result<(Self, Receiver<Vec<u8>>), PtyError>`, `process_pty_output(&[u8])`, `write_input(&[u8])`, `grid() -> &Grid`, `resize(GridSize)`, `is_alive() -> bool`.
- `arcterm-app/src/main.rs` — Tokio multi-thread runtime, `EventLoop`, `App` struct implementing `ApplicationHandler`. Resumed creates window (1024×768, "Arcterm"), constructs `Renderer`, calculates `GridSize`, spawns `Terminal`. `about_to_wait` drains PTY `try_recv` loop, calls `process_pty_output`, requests redraw. `window_event` handles `RedrawRequested`, `Resized`, `CloseRequested`, `KeyboardInput`, `ModifiersChanged`.

**Modified:**
- `arcterm-app/Cargo.toml` — added `[features]` section with `latency-trace = []`.

**Verify:** `cargo build --package arcterm-app` — PASS (1 dead-code warning for `is_alive`, not called by App directly; acceptable).

**Commit:** `480b31f shipyard(phase-1): wire PTY-VT-renderer into application shell`

---

### Task 2 — Keyboard input + cursor rendering

**New files:**
- `arcterm-app/src/input.rs` — `translate_key_event(event: &KeyEvent, modifiers: ModifiersState) -> Option<Vec<u8>>`. Covers: printable chars, Ctrl+a..z → 0x01..0x1a, Enter→`\r`, Backspace→`\x7f`, Tab→`\t`, Escape→`\x1b`, arrow keys, Home/End, PageUp/PageDown, Delete, F1-F12 (VT220 sequences), dead keys/unidentified → `None`.

**Modified:**
- `arcterm-render/src/text.rs` — `prepare_grid` now reads `grid.cursor` and applies inverse-video at the cursor cell: swaps the cell's fg color to use `ansi_color_to_glyphon(cell.attrs.bg, false)` (the background palette value as text foreground) so the cursor position visually differentiates from surrounding text.

**Deviation from plan:** The plan says "swap fg/bg colors for inverse video effect." Because glyphon renders text glyphs only (no filled background rectangles), only the glyph color can be swapped. The implemented approach swaps the text glyph color at the cursor cell to the cell's background color, producing a visual distinction that functions as a cursor indicator within the text-only renderer. A full block-cursor with background fill would require additional wgpu rectangle rendering, which is outside Phase 1 scope.

**Deviation from plan — modifier handling:** The plan's `KeyboardInput` handler description implied modifiers could be read directly from `KeyEvent`. In winit 0.30, `KeyEvent` has no `modifiers` field; modifier state is delivered via `WindowEvent::ModifiersChanged`. The `App` struct tracks `modifiers: ModifiersState` and passes it to `translate_key_event`. This is the correct winit 0.30 API pattern.

**Verify:** `cargo build --package arcterm-app` — PASS.

**Commit:** `dcc09f7 shipyard(phase-1): add keyboard input translation and cursor rendering`

---

### Task 3 — Integration testing + performance measurement

**All features implemented:**
- `latency-trace` feature flag declared in `arcterm-app/Cargo.toml`.
- When `latency-trace` is enabled, `Instant` timestamps are logged at: key received (before `translate_key_event`), PTY write bytes, PTY output processed (bytes + duration), frame submitted (duration), cold start → first frame (one-shot via `AtomicBool`).
- Shell exit handled gracefully: `TryRecvError::Disconnected` in `about_to_wait` logs an info message and requests one final redraw without panicking; no `unwrap()` on the receiver.
- `wgpu::SurfaceError::Outdated` is handled in `arcterm-render/src/renderer.rs` (already implemented in Plan 2.3 — matched with `Lost | Outdated` arm, reconfigures surface, returns early).

**Verify:**
- `cargo build --package arcterm-app` — PASS.
- `cargo build --package arcterm-app --features latency-trace` — PASS.

**Commit:** `5e05247 shipyard(phase-1): add latency tracing and error handling`

---

## Infrastructure Validation

No IaC files were modified. This plan involved Rust source files only.

---

## Final State

All three tasks verified and committed. The `arcterm-app` binary is a complete winit application shell connecting:

```
[Keyboard] → input::translate_key_event → Terminal::write_input → PtySession::write
[PTY stdout] → mpsc::Receiver → Terminal::process_pty_output → Processor::advance → Grid
[Grid + cursor] → Renderer::render_frame → TextRenderer::prepare_grid → wgpu frame
```

The application is ready for runtime testing (requires a display/GPU). No tests were skipped; no blocking issues encountered.
