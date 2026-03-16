# SUMMARY-2.1: Rewrite Terminal Wrapper with alacritty_terminal

**Plan:** PLAN-2.1
**Branch:** `phase-12-engine-swap`
**Worktree:** `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap`
**Commits:** `2ebb811`, `bec831e`
**Verification:** `cargo check -p arcterm-app && cargo test -p arcterm-app` — 322 tests pass, 0 errors

---

## What Was Done

### Task 1: ArcTermEventListener + Terminal struct (`terminal.rs` rewrite)

Replaced the old `PtySession + GridState + ApcScanner` stack with a new `Terminal` struct backed by `Arc<FairMutex<Term<ArcTermEventListener>>>`.

**Key decisions:**

- **Direct parser approach** (plan's fallback, implemented as primary): The reader thread advances `vte::ansi::Processor::<StdSyncHandler>::new()` directly against `Term`, bypassing `alacritty::EventLoop`. This was chosen because:
  1. `EventLoop` takes ownership of `Pty` and provides no hook for byte-level interception.
  2. The `from_fd` path exists but would require reassembling a `Pty` object around the write fd, adding significant complexity.
  3. Direct parser gives clean access to `PreFilter` output before Term sees any bytes.

- **ArcTermEventListener** routes:
  - `Event::Wakeup` → `wakeup_tx.send(())`
  - `Event::PtyWrite(s)` → `write_tx.try_send(s.into_bytes())`
  - `Event::ChildExit(code)` → stores `i32` in `Arc<Mutex<Option<i32>>>`
  - `Event::Title(s)` → stores in `Arc<Mutex<Option<String>>>`

- **ArcTermSize** — custom struct implementing `alacritty_terminal::grid::Dimensions`. Avoids depending on the private `term::test::TermSize` struct.

- **Writer thread** — owns the cloned PTY fd, drains `SyncSender<Cow<'static, [u8]>>` and writes to the PTY master.

- **Reader thread** — owns the original PTY fd, runs `PreFilter`, dispatches APC/OSC side channels, advances the vte parser with passthrough bytes.

- **`to_arcterm_grid()`** — bridge method converting alacritty `Term` renderable content to `arcterm_core::Grid` for the Wave-2 renderer. Removed in Plan 3.1.

- **Compatibility bridge methods**: `all_text_rows()`, `cursor_row()`, `scroll_offset()`, `set_scroll_offset()`, `bracketed_paste()`, `app_cursor_keys()`, `grid_cells_for_detect()`, `take_pending_replies()` (no-op).

### Task 2: Rewire AppState (`main.rs`)

- Removed `pty_channels: HashMap<PaneId, Receiver<Vec<u8>>>` from `AppState`.
- Replaced the 270-line PTY drain loop with wakeup-based polling:
  - `terminal.has_wakeup()` drains wakeup signals, APC payloads, OSC 7770 params, OSC 133 events.
  - Closed pane detection via `terminal.has_exited()`.
- Updated `Terminal::new()` signature at all call sites (4 locations) to `(cols, rows, cell_w, cell_h, shell, cwd) -> (Terminal, image_rx)`.
- Updated all `terminal.grid()` / `terminal.grid_mut()` / `terminal.grid_state()` call sites to new compat bridge methods.
- Fixed all `terminal.resize(GridSize)` calls to `terminal.resize(cols, rows, cell_w, cell_h)`.
- Removed all `state.pty_channels.remove(...)` calls in close/tab operations.
- Added `cell_dims() -> (u16, u16)` helper to `AppState` for resize calls before renderer initializes.
- Scrollback limit change during config reload is a no-op (alacritty configures scrollback at `Term::new()` time — tracked for Plan 3.1).

### Task 3: Wire pre-filter output into structured content pipeline (`terminal.rs`)

- **OSC 7770 text capture**: The reader thread now maintains `osc7770_capture: Option<Vec<u8>>`. When a `start` param arrives, capturing begins. Passthrough bytes are copied to the capture buffer while `osc7770_capture` is `Some`. When `end` arrives, the captured bytes are UTF-8 decoded and sent as `(params, captured_text)` via `osc7770_tx`.

- **`dispatch_osc7770(params, captured_text, completed_blocks)`**: Updated to accept the captured text. On `end`, stores it in `acc.buffer` (after `strip_ansi()` cleaning) before pushing to `completed_blocks`.

- **`strip_ansi()`**: Minimal ANSI escape stripper (CSI + OSC sequences) used to clean raw terminal output before storing as block content.

- **APC/Kitty**: Wired via `apc_rx` in `has_wakeup()` → `process_kitty_payload()` → `spawn_blocking` decode → `image_tx`.

- **OSC 133**: Wired via `osc133_rx` in `has_wakeup()` → `osc133_events` vec → `take_exit_codes()` drains `CommandFinished` variants into `PaneContext`.

---

## Deviations from Plan

| Plan Spec | Actual Implementation | Reason |
|---|---|---|
| Pipe-based EventLoop approach as primary | Direct parser as primary | `EventLoop` takes ownership of `Pty`; no public hook for byte interception. Plan explicitly allowed this fallback. |
| `loop_sender: EventLoopSender` in Terminal | `write_tx: SyncSender<Cow<[u8]>>` + writer thread | EventLoop bypassed; writer thread replaces EventLoopSender. |
| `has_wakeup() -> bool` via single channel | Drains 4 channels: wakeup, apc, osc7770, osc133 | Combines all side-channel draining into one call to reduce main-loop complexity. |
| Update `selection.rs` to work without `&Grid` | `to_arcterm_grid()` bridge keeps `selection.extract_text(&Grid)` working | Minimal-change approach to avoid breaking selection tests in this plan. Wave 3 Plan 3.1 removes this bridge. |

---

## Files Modified

- `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/terminal.rs` — full rewrite (~1000 lines new)
- `/Users/lgbarn/Personal/arcterm/.worktrees/phase-12-engine-swap/arcterm-app/src/main.rs` — ~580 lines changed

## Warnings (5, pre-existing or benign)

1. `CellAttrs` unused import in `to_arcterm_grid` — cosmetic, `CellAttrs` struct exists in scope but only `fg`/`bg`/`bold`/`italic`/`underline` fields are set directly.
2. `ring_capacity` field never read in `context.rs` — pre-existing.
3. `push_output_line` never used in `context.rs` — pre-existing.
4. `take_pending_replies` never called — it's a compat no-op; Wave 4 can remove it.
5. `MAXPATHLEN` unused in macOS `cwd_for_pid` — constant defined for documentation; accepted.
