# Plan 2.1: Rewrite Terminal Wrapper with alacritty_terminal

## Context

This is the core of the migration. The current `Terminal` struct in `arcterm-app/src/terminal.rs` wraps `PtySession` + `GridState` + `ApcScanner`. It must be replaced with a wrapper around alacritty's `Arc<FairMutex<Term<T>>>` + `EventLoopSender` + the new PreFilter.

The new `Terminal` must provide the same public API surface that `AppState` (in main.rs) uses: `write_input`, `resize`, `child_pid`, `cwd`, `grid` access for rendering, and drain methods for structured content, exit codes, tool queries, and tool calls.

This plan also implements the `EventListener` trait and wires the pipe-based pre-filter architecture.

## Dependencies

- Plan 1.1 (alacritty_terminal dependency available, bridge types relocated)
- Plan 1.2 (PreFilter module built and tested)

## Tasks

### Task 1: Implement EventListener and Terminal struct
**Files:** `arcterm-app/src/terminal.rs` (full rewrite)
**Action:** refactor
**Description:**
Rewrite `terminal.rs` completely. The new module must contain:

**1. ArcTermEventListener** — implements `alacritty_terminal::event::EventListener`:
```rust
pub struct ArcTermEventListener {
    wakeup_tx: std::sync::mpsc::Sender<()>,
    loop_sender: Option<EventLoopSender>,
}
```
- `send_event(Event::Wakeup)` → sends `()` on `wakeup_tx` (signals main thread to redraw)
- `send_event(Event::PtyWrite(s))` → `loop_sender.send(Msg::Input(s.into_bytes().into()))` (writes DSR/DA replies back to PTY)
- `send_event(Event::ChildExit(status))` → stores exit status in an `Arc<Mutex<Option<ExitStatus>>>`
- `send_event(Event::Title(s))` → stores title in an `Arc<Mutex<Option<String>>>`
- Other variants: log and ignore for now

**2. Terminal struct**:
```rust
pub struct Terminal {
    term: Arc<FairMutex<Term<ArcTermEventListener>>>,
    loop_sender: EventLoopSender,
    child_pid: u32,
    prefilter: PreFilter,
    // Side channels from PreFilter output:
    osc7770_accumulator: Vec<StructuredContentAccumulator>,
    osc133_events: Vec<Osc133Event>,
    chunk_assembler: KittyChunkAssembler,
    image_tx: mpsc::Sender<PendingImage>,
    // Shared state from EventListener:
    exit_status: Arc<Mutex<Option<ExitStatus>>>,
    title: Arc<Mutex<Option<String>>>,
    wakeup_rx: std::sync::mpsc::Receiver<()>,
}
```

**3. Terminal::new()** — the construction sequence:
1. Create `alacritty_terminal::tty::Options` from shell/cwd params
2. Create `WindowSize` from the grid size + cell dimensions
3. Call `tty::new(&options, window_size, 0)` to get a `Pty`
4. Extract `child_pid = pty.child().id()` BEFORE EventLoop takes ownership
5. Create the Unix pipe pair (read_end, write_end) for the pre-filter
6. Clone `pty.file()` (the raw PTY fd) for the pre-filter reader
7. Create `Term::new(config, &size, event_listener)` wrapped in `Arc<FairMutex<_>>`
8. Create `EventLoop::new(term.clone(), pty_with_pipe_read_end, ...)` — NOTE: the Pty given to EventLoop reads from the pipe read-end, not the raw PTY fd
9. Spawn the EventLoop thread via `event_loop.spawn()`
10. Spawn a `std::thread` (or tokio task) for the pre-filter: reads from the cloned raw PTY fd, runs `prefilter.advance()`, writes passthrough bytes to the pipe write-end, dispatches APC/OSC to channels
11. Return `(Terminal, wakeup_rx, image_rx)`

**Important design note on the pipe approach:** The research identified that `EventLoop::new` takes ownership of a `Pty` object. To use the pipe approach, we need to investigate whether alacritty has a `tty::from_fd` or similar constructor. If not, the alternative is to bypass EventLoop entirely and drive `vte::Parser` + `Term` directly (the research's Option 1). The implementer should try the pipe approach first; if `from_fd` is not available, fall back to the direct parser approach:
- Create a `vte::Parser` manually
- In the pre-filter thread: read from PTY fd, run PreFilter, then call `parser.advance(&mut *term.lock(), &passthrough_bytes)` directly
- Handle write-back via a separate channel (replacing EventLoopSender)

**4. Public methods** (matching the current API surface consumed by AppState):
- `write_input(&self, data: &[u8])` → `self.loop_sender.send(Msg::Input(data.into()))`
- `resize(&self, cols: usize, rows: usize, cell_w: u16, cell_h: u16)` → `self.loop_sender.send(Msg::Resize(WindowSize{...}))`
- `child_pid() -> Option<u32>` → `Some(self.child_pid)`
- `cwd() -> Option<PathBuf>` → readlink `/proc/{pid}/cwd` (same logic as current PtySession::cwd)
- `lock_term() -> FairMutexGuard<Term<...>>` → `self.term.lock()` (for renderer access)
- `take_completed_blocks() -> Vec<StructuredContentAccumulator>` → drain from osc7770 accumulator
- `take_exit_codes() -> Vec<i32>` → drain from osc133_events (filter CommandFinished variants)
- `take_tool_queries() -> Vec<()>` → drain from osc7770 accumulator (tools/list type)
- `take_tool_calls() -> Vec<(String, String)>` → drain from osc7770 accumulator (tools/call type)
- `take_context_queries() -> Vec<()>` → drain from osc7770 accumulator (context/query type)
- `has_wakeup() -> bool` → `self.wakeup_rx.try_recv().is_ok()` (non-blocking check)

**Acceptance Criteria:**
- `cargo check -p arcterm-app` succeeds (the new Terminal compiles)
- The `Terminal::new()` function creates a PTY, extracts child PID, sets up EventLoop or direct parser, and returns successfully
- The pre-filter thread is spawned and reads from the PTY fd

### Task 2: Rewire AppState to use new Terminal API
**Files:** `arcterm-app/src/main.rs`
**Action:** modify
**Description:**
Update `AppState` and the `about_to_wait` function to use the new Terminal:

1. **Remove `pty_channels: HashMap<PaneId, mpsc::Receiver<Vec<u8>>>`** — PTY byte streams are no longer externally channeled. The EventLoop (or pre-filter thread) feeds bytes directly into Term.

2. **Replace the PTY drain loop** — The current `about_to_wait` iterates `pty_channels`, calls `try_recv`, and feeds bytes to `terminal.process_pty_output()`. Replace this with:
   - Check `terminal.has_wakeup()` for each pane
   - If wakeup received, the Term already has new data (EventLoop processed it)
   - Drain structured content: `terminal.take_completed_blocks()`, `take_exit_codes()`, etc.
   - Process Kitty images from the pre-filter's APC channel (similar to current image pipeline)

3. **Update Terminal::new() call sites** — The function signature changes. Update all places that call `Terminal::new()`:
   - Initial pane creation (~line 347)
   - `add_pane()` method (~line 841)
   - Workspace restore pane creation (~line 923, ~line 1087)

   The new signature returns `(Terminal, wakeup_rx, image_rx)` or similar — adjust the channel maps accordingly.

4. **Update `write_input` calls** — Currently `terminal.write_input(data)`. The new API is the same signature but backed by EventLoopSender.

5. **Update `resize` calls** — Currently `terminal.resize(GridSize)`. The new API takes `(cols, rows, cell_w, cell_h)` to construct a `WindowSize`.

6. **Update `grid()` access** — Currently `terminal.grid()` returns `&Grid`. The new API requires `terminal.lock_term()` to get a guard, then `guard.grid()`. This is used in the render path (which will be updated in Plan 3.1) but also in selection.rs, detect.rs, and context-gathering code. For now, add a convenience method `Terminal::with_term<R>(&self, f: impl FnOnce(&Term<...>) -> R) -> R` that locks, calls the closure, and unlocks.

7. **Remove `use arcterm_core::*` and `use arcterm_pty::*` from main.rs** — Replace with the new types. `GridSize` is no longer needed (use `WindowSize` or `(cols, rows)`). `Color`, `Cell`, `CellAttrs`, `CursorPos` are no longer directly referenced in main.rs (they move to the renderer bridge in Wave 3).

8. **Update `selection.rs`** — It currently uses `arcterm_core::{Cell, Grid}`. Add a bridge: the selection logic needs access to cell characters and positions. Either pass a snapshot of the relevant data, or update selection to work with alacritty's `Grid<Cell>` type. The simplest approach: `selection.rs` functions take `&str` (extracted text) rather than `&Grid`.

9. **Update `detect.rs`** — It uses `arcterm_core::Cell` for auto-detection. Update to work with alacritty's `Cell` type or extract the text as a `String` before passing to detect.

**Acceptance Criteria:**
- `cargo check -p arcterm-app` succeeds with zero errors
- `cargo test -p arcterm-app` passes (tests may need updating for new types)
- AppState can create a Terminal, receive wakeup signals, and drain structured content
- `write_input` sends bytes to the PTY via EventLoopSender
- Resize propagates to both Term and PTY

### Task 3: Wire pre-filter output into structured content pipeline
**Files:** `arcterm-app/src/terminal.rs`, `arcterm-app/src/main.rs`
**Action:** modify
**Description:**
Connect the pre-filter's side channels to the existing structured content and AI feature pipelines:

1. **OSC 7770 parsing**: The pre-filter emits raw parameter strings like `"start;type=code;lang=rs"` or `"end"` or `"tools/list"`. Add a `parse_osc7770_params(params: &str)` function in `osc7770.rs` that:
   - For `start;type=X;...` → creates a new `StructuredContentAccumulator` and pushes it onto the active accumulator stack
   - For `end` → pops the active accumulator, marks it complete, adds to completed_blocks
   - For `tools/list` → adds to tool_queries
   - For `tools/call;name=X;args=Y` → adds to tool_calls
   - For `context/query` → adds to context_queries
   This mirrors the dispatch logic currently in `arcterm-vt/src/processor.rs` Osc7770Dispatcher.

2. **OSC 133 parsing**: The pre-filter emits `Osc133Event` variants. In `about_to_wait`, drain these and:
   - `CommandFinished(Some(code))` → store as last exit code in PaneContext (same as current `take_exit_codes`)

3. **APC/Kitty processing**: The pre-filter emits raw APC payloads. Process them through `parse_kitty_command` + `KittyChunkAssembler` + `image_tx` (same pipeline as current `process_pty_output`, just with the pre-filter as the source).

4. **Text accumulation for OSC 7770 blocks**: Currently, the VT handler's `put_char` appends to the active accumulator's buffer. With alacritty, the text goes directly to Term's grid — there's no hook. Instead, the pre-filter must also capture the text between `OSC 7770 start` and `OSC 7770 end` markers. This requires the pre-filter to have a "capturing" mode where passthrough bytes (between start and end) are both forwarded to EventLoop AND copied to the active accumulator's buffer. Update the PreFilter to support this.

**Acceptance Criteria:**
- OSC 7770 structured content blocks are correctly accumulated and available via `take_completed_blocks()`
- OSC 133 exit codes are captured and available via `take_exit_codes()`
- Kitty APC payloads are processed through the chunk assembler and delivered to `image_rx`
- `cargo test -p arcterm-app` passes

## Verification

```bash
cargo check -p arcterm-app && cargo test -p arcterm-app
```

The arcterm-app crate compiles and tests pass. The Terminal wrapper creates a functional PTY session backed by alacritty_terminal. The pre-filter intercepts OSC 7770/133/APC sequences. AppState can drive the full lifecycle: create terminal, receive wakeup, drain structured content, write input, resize.
