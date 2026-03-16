# Research: Phase 12 — alacritty_terminal Migration

## Context

Arcterm is a GPU-accelerated terminal emulator in Rust (edition 2024) that currently builds on three custom crates: `arcterm-core` (grid and cell types), `arcterm-vt` (VT parser wrapping `vte`), and `arcterm-pty` (PTY wrapping `portable-pty`). The renderer in `arcterm-render` reads directly from `arcterm-core::Grid`. Phase 12 replaces all three crates with `alacritty_terminal`, which provides a complete, battle-tested terminal emulation stack under a single dependency.

This research establishes the exact API surface of `alacritty_terminal` 0.25.1, maps each existing arcterm integration point to its replacement, identifies dependency conflicts, and documents risks.

---

## Comparison Matrix

| Criteria | arcterm-core + arcterm-vt + arcterm-pty | alacritty_terminal 0.25.1 |
|----------|-----------------------------------------|---------------------------|
| Maturity | ~1 year, custom | ~8 years, production terminal |
| Version | 0.1.0 (workspace) | 0.25.1 (released 2025-10-18) |
| License | MIT | Apache-2.0 |
| VT conformance | Partial (custom vte wrapper) | Very high (used in Alacritty, Zellij) |
| PTY support | portable-pty + custom wrapper | Built-in tty module (Unix + Windows) |
| Grid type | `Vec<Vec<Cell>>` (custom) | `Grid<Cell>` (production-grade) |
| Scrollback | Custom VecDeque | Built-in, configurable via `Config::scrolling_history` |
| OSC hook | Custom handler dispatch | Silent-drop for unknown OSCs (must pre-filter) |
| APC/Kitty | Custom ApcScanner pre-filter | Silent-drop (must pre-filter) |
| crates.io downloads | N/A (local) | 496,872 total, ~69,452 recent |
| Dependency count | 3 crates + vte + portable-pty | 1 crate (replaces all 5) |
| Maintenance | Arcterm team | Alacritty project (active) |

Sources:
- https://crates.io/api/v1/crates/alacritty_terminal (accessed 2026-03-16)
- https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/ (accessed 2026-03-16)

---

## alacritty_terminal API Overview

**Version:** 0.25.1 (published 2025-10-18). The development tree shows 0.25.2-dev in `Cargo.toml`; crates.io latest stable is 0.25.1.

**License:** Apache-2.0. This is compatible with arcterm's MIT license for binary distribution, but the project should note the license difference if distributing source.

**Top-level public API:**

```
alacritty_terminal::
  Term<T>                   -- the terminal; generic over EventListener
  Grid<T>                   -- re-exported from grid module
  vte                       -- re-exported vte crate

  event::
    EventListener (trait)   -- fn send_event(&self, Event)
    Event (enum)            -- 13 variants
    WindowSize (struct)     -- num_lines, num_cols, cell_width, cell_height (all u16)

  event_loop::
    EventLoop<T, U>         -- runs PTY I/O on a dedicated thread
    EventLoopSender         -- sends Msg to the event loop
    Msg (enum)              -- Input(Cow<'static,[u8]>), Shutdown, Resize

  grid::
    Grid<T>                 -- 2D storage; cursor, saved_cursor fields
    GridIterator<'a,T>      -- yields Indexed<&'a T>
    Indexed<T>              -- .point() -> Point, .cell() -> &T
    Dimensions (trait)      -- columns(), screen_lines(), total_lines()
    Scroll (enum)           -- Delta(i32), PageUp, PageDown, Top, Bottom

  index::
    Point<L,C>              -- .line: L, .column: C; Point::new(line, col)
    Line(i32)               -- newtype; negative = scrollback, 0 = viewport top
    Column(usize)           -- newtype

  selection::
    Selection               -- text selection state
    SelectionRange          -- start/end Points

  sync::
    FairMutex<T>            -- fair mutex; Term<T> is held in Arc<FairMutex<Term<T>>>

  term::
    Term<T>                 -- terminal state machine
    Config                  -- scrolling_history, default_cursor_style, etc.
    RenderableContent<'a>   -- display_iter, cursor, selection, display_offset, colors, mode
    RenderableCursor        -- shape: CursorShape, point: Point
    TermMode                -- bitflags (SHOW_CURSOR, ALT_SCREEN, BRACKETED_PASTE, ...)
    cell::
      Cell                  -- c: char, fg: Color, bg: Color, flags: Flags, extra: Option<Arc<CellExtra>>
      Flags                 -- BOLD, ITALIC, DIM, UNDERLINE, DOUBLE_UNDERLINE, UNDERCURL,
                               DOTTED_UNDERLINE, DASHED_UNDERLINE, ALL_UNDERLINES,
                               INVERSE, HIDDEN, STRIKEOUT, WRAPLINE, WIDE_CHAR,
                               WIDE_CHAR_SPACER, LEADING_WIDE_CHAR_SPACER, BOLD_ITALIC, DIM_BOLD
    color::
      Colors                -- indexed array, Colors[usize] and Colors[NamedColor] -> Option<Rgb>
      Rgb                   -- r, g, b fields (u8 each); from term::color module
    test::
      TermSize              -- columns: usize, screen_lines: usize; implements Dimensions

  tty::
    Pty                     -- .child() -> &Child, .file() -> &File
                               implements EventedReadWrite (.reader(), .writer() -> &mut File)
                               implements OnResize (.on_resize(WindowSize))
    Options                 -- shell: Option<Shell>, working_directory: Option<PathBuf>,
                               drain_on_exit: bool, env: HashMap<String,String>
    Shell                   -- shell path + arguments
    new(config: &Options, window_size: WindowSize, window_id: u64) -> Result<Pty>
```

Source: https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/ (accessed 2026-03-16)

---

## Grid & Cell API

### Creating a Term

```rust
// 1. Implement Dimensions (or use TermSize from term::test)
use alacritty_terminal::term::test::TermSize;
let size = TermSize { columns: 80, screen_lines: 24 };

// 2. Create config
let config = alacritty_terminal::term::Config {
    scrolling_history: 10_000,
    ..Default::default()
};

// 3. Term::new requires Arc<FairMutex<Term<U>>> pattern for EventLoop
let term = Term::new(config, &size, my_event_listener);
let term = Arc::new(FairMutex::new(term));
```

### Processing Bytes

The `EventLoop` processes PTY bytes automatically — it reads from `Pty.reader()`, calls `state.parser.advance(&mut **terminal, &buf)` internally, and sends `Event::Wakeup` when new data is ready. **There is no public `Term::process(&[u8])` method.** Bytes reach the `Term` only through the `EventLoop`.

This is the most significant architectural difference from the current arcterm design, where `Terminal::process_pty_output(&[u8])` is called directly. The replacement design must use the `EventLoop` thread pattern, or manually invoke the vte parser (see Migration Risk #1).

### Reading the Grid for Rendering

```rust
let term = arc_term.lock();
let content = term.renderable_content();

// Iterate visible cells:
for indexed_cell in content.display_iter {
    let row = indexed_cell.point.line;    // Line (i32 newtype, 0 = viewport top)
    let col = indexed_cell.point.column;  // Column (usize newtype)
    let cell = &*indexed_cell;            // deref to &Cell (Indexed<&Cell> implements Deref)
    // or: indexed_cell.cell() if that method exists
}

// Cursor:
let cursor_row = content.cursor.point.line;     // Line
let cursor_col = content.cursor.point.column;   // Column
let cursor_shape = content.cursor.shape;        // CursorShape

// Mode flags:
let cursor_visible = content.mode.contains(TermMode::SHOW_CURSOR);
let alt_screen = content.mode.contains(TermMode::ALT_SCREEN);
```

### Cell Attributes Mapping

| arcterm-core CellAttrs field | alacritty_terminal Cell field |
|------------------------------|-------------------------------|
| `cell.c` (char) | `cell.c` (char) |
| `cell.attrs.fg` (Color enum) | `cell.fg` (vte::ansi::Color) |
| `cell.attrs.bg` (Color enum) | `cell.bg` (vte::ansi::Color) |
| `cell.attrs.bold` (bool) | `cell.flags.contains(Flags::BOLD)` |
| `cell.attrs.italic` (bool) | `cell.flags.contains(Flags::ITALIC)` |
| `cell.attrs.underline` (bool) | `cell.flags.contains(Flags::UNDERLINE)` |
| `cell.attrs.reverse` (bool) | `cell.flags.contains(Flags::INVERSE)` |
| `cell.dirty` (bool) | Not present; use `TermDamage` instead |

**Color type change:** arcterm-core uses `Color { Default, Indexed(u8), Rgb(u8,u8,u8) }`. Alacritty uses `vte::ansi::Color`. The vte Color enum variants need to be verified by fetching its source (docs.rs 404'd for the specific URL), but the Cell documentation confirms `fg: Color` and `bg: Color` where `Color` is from `vte` 0.15.0. The renderer's `ansi_color_to_glyphon` function will need to be updated to match the vte Color enum shape.

**Note on `display_iter` line indexing:** `Line` is an `i32` newtype where `0` is the first visible line and negative values are scrollback. This differs from arcterm-core's `usize` row indexing. The renderer loop will need to convert `Line` to a `usize` row index offset.

### Scrollback

Set `Config::scrolling_history` at construction. Scroll with `term.scroll_display(Scroll::Delta(n))`. The `content.display_offset` field from `renderable_content()` reflects the current viewport position.

### Resize

```rust
// Resize the Term (no direct Term::resize with WindowSize — uses Dimensions)
// Via EventLoop (preferred):
loop_sender.send(Msg::Resize(new_window_size)).unwrap();

// The Pty also has on_resize:
pty.on_resize(new_window_size);  // sets TIOCSWINSZ
```

Source: https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/term/struct.Term.html (accessed 2026-03-16)

---

## PTY API

### Creating a PTY

```rust
use alacritty_terminal::tty::{self, Options, Shell, WindowSize};
use alacritty_terminal::event::WindowSize;

let options = Options {
    shell: Some(Shell { program: "/bin/zsh".into(), args: vec![] }),
    working_directory: Some(PathBuf::from("/home/user")),
    drain_on_exit: false,
    env: HashMap::new(),
};
let window_size = WindowSize {
    num_lines: 24,
    num_cols: 80,
    cell_width: 8,   // pixels; 0 acceptable if unknown
    cell_height: 16, // pixels; 0 acceptable if unknown
};
let pty = tty::new(&options, window_size, window_id)?;
```

`window_id` is a `u64` passed to the PTY for environment variable `$WINDOWID`. Use `0` or a hash of the pane ID for multi-pane.

### Reading/Writing

The `EventLoop` owns the `Pty` after construction and handles all reads/writes internally. To write input bytes (keyboard), use `EventLoopSender::send(Msg::Input(bytes))`. To resize, send `Msg::Resize(window_size)`.

**Child PID:** `pty.child().id()` returns the child process PID as `u32`. After `EventLoop::new()` takes ownership of the `Pty`, PID access requires holding a reference to the `Pty` before passing it to `EventLoop`, or querying `/proc/{pid}` directly as arcterm currently does in `ai_detect.rs`.

### Getting Child PID (design issue)

The current arcterm design uses `Terminal::child_pid()` which calls `PtySession::child_pid()` extensively in `ai_detect.rs` and `context.rs`. After handing `Pty` to `EventLoop`, the `Pty` is owned by the event loop thread. The child PID must be extracted before `EventLoop::new()` is called and stored separately.

Source: https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/tty/fn.new.html (accessed 2026-03-16)

---

## Event System

### EventListener Trait

```rust
pub trait EventListener {
    fn send_event(&self, _event: Event) {}
}
```

One method with a default no-op implementation. Arcterm must implement this to receive `Event::Wakeup` (triggers a redraw), `Event::PtyWrite` (writes response bytes back to PTY — used for DSR/DA replies), `Event::Title`, `Event::Bell`, `Event::ChildExit(ExitStatus)`, etc.

### Event Variants (full list)

| Variant | Description | Arcterm relevance |
|---------|-------------|-------------------|
| `Wakeup` | New PTY data processed, redraw needed | Replaces the `pty_rx` channel mechanism; must trigger `about_to_wait` |
| `PtyWrite(String)` | Write text back to PTY | Replaces `grid.pending_replies` drain mechanism |
| `Title(String)` | Shell set window title | Can update pane title |
| `ResetTitle` | Reset to default title | Minor UI update |
| `ClipboardStore(ClipboardType, String)` | OSC 52 copy | Replaces current clipboard handling |
| `ClipboardLoad(ClipboardType, Arc<dyn Fn(&str)->String>)` | OSC 52 paste | Replaces current clipboard handling |
| `ColorRequest(usize, Arc<dyn Fn(Rgb)->String>)` | Color query | Low priority |
| `TextAreaSizeRequest(Arc<dyn Fn(WindowSize)->String>)` | Size query | Low priority |
| `CursorBlinkingChange` | Cursor blink state changed | UI hint |
| `MouseCursorDirty` | Grid changed, recheck cursor shape | Minor |
| `Bell` | Terminal bell | Low priority |
| `Exit` | Shutdown request | Needed for shell exit handling |
| `ChildExit(ExitStatus)` | Child process exited | Replaces current exit code tracking |

**Critical:** `Event::PtyWrite(String)` replaces the `take_pending_replies()` drain in the current `Terminal` struct. Whenever alacritty processes a DSR query (cursor position report, device attributes, etc.), it dispatches `Event::PtyWrite` rather than buffering. The `EventListener` implementation must write these bytes to the PTY immediately via `EventLoopSender::send(Msg::Input(...))`.

**Critical:** `Event::Wakeup` does not carry the rendered content — it is just a signal. After receiving `Wakeup`, arcterm must lock `Arc<FairMutex<Term<T>>>` and call `term.renderable_content()`.

Source: https://github.com/alacritty/alacritty/blob/master/alacritty_terminal/src/event.rs (accessed 2026-03-16)

---

## OSC & Custom Sequence Handling

### How alacritty handles OSC sequences

The `vte` crate parses the byte stream and calls `Handler::osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool)` on `Term`. The `Term` implementation dispatches on the OSC code for sequences it knows:

- OSC 0/1/2: window title (`set_title`)
- OSC 4/10/11/12/17/19: color queries (`dynamic_color_sequence`, `set_color`, `reset_color`)
- OSC 52: clipboard (`clipboard_store`, `clipboard_load`)

**Unknown OSC sequences (including OSC 7770) are silently dropped.** There is no extensibility hook, callback registration, or fallthrough mechanism in `Term`'s `osc_dispatch`.

### Implication for OSC 7770

The pre-filter (D4 from CONTEXT-12.md) must intercept OSC 7770 sequences **before** bytes reach alacritty's `EventLoop`. Since the `EventLoop` owns the PTY file descriptor and reads from it directly, the pre-filter must run on the same thread as the `EventLoop` or intercept at the byte level before the event loop processes the stream.

**This is a fundamental architectural constraint.** The `EventLoop` does not offer a byte interception hook. Two viable approaches exist:

1. **Not using `EventLoop`:** Read from `Pty.reader()` directly in a custom tokio task, run the pre-filter, then call `parser.advance(&mut term, filtered_bytes)` manually. This requires holding a `vte::Parser` and driving `Term` as a `vte::Perform` implementor. This approach bypasses `EventLoop` but loses its write buffering and polling logic.

2. **Pipe-based pre-filter:** Create a Unix pipe, have a pre-filter task read from `Pty.reader()`, strip/dispatch OSC 7770 and APC sequences, and write clean bytes to the pipe's write end. Give `EventLoop` the pipe's read end (using `tty::from_fd`). This preserves the `EventLoop` architecture.

Both approaches have complexity. Option 2 (pipe-based) is more aligned with D2 ("use alacritty's full PTY module") but requires `from_fd`. Option 1 (bypass EventLoop) is simpler but requires reimplementing EventLoop's write path.

Source: https://github.com/alacritty/alacritty/blob/master/alacritty_terminal/src/term/mod.rs (accessed 2026-03-16)

---

## Kitty Graphics Support

Alacritty does **not** implement the Kitty graphics protocol (APC sequences `ESC _ G … ESC \`). The `vte` crate's parser does not have a callback for APC sequences that `Term` implements — APC sequences are silently ignored by the vte state machine's default path.

The existing `ApcScanner` in `arcterm-vt/src/processor.rs` is the correct model for Phase 12. The Phase 12 pre-filter must:

1. Intercept APC sequences (`ESC _ … ESC \`) — extract Kitty graphics payloads
2. Intercept OSC 7770 sequences (`ESC ] 7770 ; … BEL|ST`) — dispatch to arcterm handlers
3. Pass all other bytes to alacritty unchanged

The pre-filter state machine will need to track both APC state (current `ApcScanner`) and OSC state (new, modeled on `ApcScanner`). The existing scanner's approach of batching non-special bytes into passthrough slices is the right performance strategy.

Source: Code analysis of `/Users/lgbarn/Personal/arcterm/arcterm-vt/src/processor.rs` + alacritty_terminal documentation

---

## Current Arcterm Integration Points

The following table maps each existing integration point to what must change.

### arcterm-app/src/terminal.rs

| Current field/method | Current type | Replacement |
|----------------------|--------------|-------------|
| `pty: PtySession` | arcterm-pty | `Arc<FairMutex<Term<E>>>` + `EventLoopSender` |
| `scanner: ApcScanner` | arcterm-vt | New `PreFilter` (OSC 7770 + APC intercept) |
| `grid_state: GridState` | arcterm-vt | Removed; `Term` manages grid |
| `chunk_assembler: KittyChunkAssembler` | arcterm-vt | Kept — Kitty assembly is still needed |
| `process_pty_output(&mut self, bytes: &[u8])` | calls `scanner.advance` | Removed; replaced by EventLoop + EventListener |
| `take_pending_replies()` | drains `grid.pending_replies` | Replaced by `Event::PtyWrite` in EventListener |
| `take_completed_blocks()` | OSC 7770 structured content | Replaced by pre-filter output channel |
| `take_exit_codes()` | OSC 133 D exit codes | Replaced by pre-filter + `Event::ChildExit` |
| `take_tool_queries()` | OSC 7770 tool queries | Replaced by pre-filter output channel |
| `take_tool_calls()` | OSC 7770 tool calls | Replaced by pre-filter output channel |
| `take_context_queries()` | OSC 7770 context queries | Replaced by pre-filter output channel |
| `write_input(&mut self, data: &[u8])` | calls `pty.write(data)` | `loop_sender.send(Msg::Input(bytes.into()))` |
| `grid(&self) -> &Grid` | arcterm-core Grid | `arc_term.lock().grid()` — alacritty Grid |
| `grid_mut(&mut self)` | arcterm-core Grid | Lock term, operate, unlock |
| `set_scroll_offset(usize)` | custom | `term.scroll_display(Scroll::Delta(n))` |
| `resize(GridSize)` | arcterm-core GridSize | `loop_sender.send(Msg::Resize(window_size))` |
| `is_alive() -> bool` | PtySession::is_alive | Implicit via `Event::ChildExit` |
| `child_pid() -> Option<u32>` | PtySession::child_pid | Extract from `pty.child().id()` before EventLoop::new; store in Terminal struct |
| `cwd() -> Option<PathBuf>` | PtySession::cwd | Keep same `/proc/{pid}/cwd` readlink logic |

### arcterm-render/src/renderer.rs and text.rs

`PaneRenderInfo` holds `grid: &Grid` where `Grid` is currently `arcterm_core::Grid`. This type must change to `alacritty_terminal::Grid<alacritty_terminal::term::cell::Cell>`.

The renderer function `build_quad_instances_at(grid: &Grid, ...)` iterates `grid.rows_for_viewport()` which returns `&[&[arcterm_core::Cell]]`. The replacement uses `term.renderable_content().display_iter` which yields `Indexed<&Cell>` (a flat iterator, not rows of slices).

The renderer loop structure will change from:
```
for (row_idx, row) in rows.iter().enumerate() {
    for (col_idx, cell) in row.iter().enumerate() { ... }
}
```
to:
```
for indexed in content.display_iter {
    let row = indexed.point.line.0;   // i32 → row number
    let col = indexed.point.column.0; // usize
    let cell = &*indexed;
}
```

The `shape_row_into_buffer` function signature takes `row: &[arcterm_core::Cell]`. It must be rewritten to accept either an iterator or a slice of `alacritty_terminal::term::cell::Cell`.

The `ansi_color_to_glyphon` function converts `arcterm_core::Color` to glyphon Color. It must be updated to convert `vte::ansi::Color` (alacritty's cell color type). The vte Color enum is expected to have variants matching arcterm-core's `Color` (Default, Indexed(u8), RGB). Exact variant names need code verification (docs.rs 404'd for the specific enum page).

### arcterm-app/src/context.rs

`collect_sibling_contexts` takes `panes: &HashMap<PaneId, Terminal>` and calls `t.cwd()`. This function signature is unchanged — `Terminal` is the new wrapper struct. The `cwd()` implementation moves from `PtySession::cwd()` to using the stored child PID with `proc` crate lookup.

### arcterm-app/src/ai_detect.rs

`detect_ai_agent(pid: u32)` and `AiAgentState::check(pid: Option<u32>)` are pure functions that take a PID and call `process_comm()` / `process_args()` from `proc.rs`. These do not directly depend on `arcterm-pty`. They remain unchanged. The only change is how the PID is obtained: from the stored `child_pid: u32` extracted before `EventLoop` takes ownership of `Pty`.

### arcterm-app/src/main.rs (AppState, about_to_wait)

The current `about_to_wait` loop drains `pty_rx: HashMap<PaneId, mpsc::Receiver<Vec<u8>>>` and calls `terminal.process_pty_output(bytes)`. With alacritty's EventLoop, this channel-based approach changes: `Event::Wakeup` (delivered via `EventListener::send_event`) signals that the `Term` has new data. The `about_to_wait` function must instead lock the `Term` and call `renderable_content()`.

This is a significant control flow change. The current tokio `mpsc` byte channel between PTY reader and main loop is replaced by an `EventListener` callback that posts a wakeup signal, which the winit event loop polls via `EventLoopProxy` or a channel.

---

## Dependency Compatibility Notes

### alacritty_terminal's dependencies vs arcterm's workspace

| alacritty_terminal dep | Version | Arcterm workspace dep | Conflict? |
|------------------------|---------|----------------------|-----------|
| `vte` | 0.15.0 | `vte = "0.15"` | None — exact match |
| `base64` | 0.22.0 | `base64 = "0.22"` | None — exact match |
| `log` | 0.4 | `log = "0.4"` | None |
| `bitflags` | 2.4.1 | Not in workspace | None (additive) |
| `parking_lot` | 0.12.0 | Not in workspace | None (additive) |
| `polling` | 3.8.0 | Not in workspace | None (additive) |
| `regex-automata` | 0.4.3 | Not in workspace | None (additive) |
| `unicode-width` | 0.2.0 | Not in workspace | None (additive) |
| `serde` | 1.x | `serde = "1"` in arcterm-app | None |
| `libc` | 0.2 | `libc = "0.2"` in arcterm-app, arcterm-pty | None |
| `rustix` | 1.0.0 (Unix) | Not in workspace | None (additive) |
| `signal-hook` | 0.4.3 (Unix) | Not in workspace | None (additive) |
| `wgpu` | Not used | `wgpu = "28"` | None (alacritty has no GPU deps) |
| `winit` | Not used | `winit = "0.30"` | None |
| `tokio` | Not used | `tokio = "1"` | None — alacritty uses raw threads |

**No dependency version conflicts exist.** The `vte = "0.15"` match is important: it means the `vte::ansi::Color` and `vte::ansi::Flags` types used by alacritty's `Cell` are the same crate version already in the workspace.

**`portable-pty`** will be removed from the workspace when `arcterm-pty` is deleted. No other workspace member depends on it.

**Thread model mismatch:** Alacritty's `EventLoop` runs on a raw OS thread (not a tokio task). This is compatible with arcterm's tokio runtime — the EventLoop thread posts events back via `EventListener::send_event` which can use a `tokio::sync::mpsc::Sender` or an `std::sync::mpsc::Sender` to communicate with the tokio runtime.

Source: Cargo.toml at https://raw.githubusercontent.com/alacritty/alacritty/master/alacritty_terminal/Cargo.toml (accessed 2026-03-16)

---

## Migration Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| EventLoop owns Pty; no byte-level interception hook | High | High | Use pipe-based pre-filter: pre-filter reads raw PTY, writes clean bytes to a pipe, give pipe read-end to EventLoop via `tty::from_fd` |
| OSC 7770 silently dropped by alacritty | Certain (by design) | High | Pre-filter intercepts before EventLoop sees bytes; this is D4 |
| Kitty APC silently dropped by alacritty | Certain | High | Pre-filter intercepts APC; existing ApcScanner logic is the model |
| `Line` type is `i32`; renderer uses `usize` row indices | Certain | Medium | Renderer bridge converts `line.0 as usize` with display_offset subtraction |
| `Event::PtyWrite` requires immediate write-back via EventLoopSender | Certain | Medium | EventListener impl calls `sender.send(Msg::Input(...))` synchronously |
| Child PID inaccessible after EventLoop::new | Certain | Medium | Extract `pty.child().id()` before passing Pty to EventLoop; store in Terminal struct |
| TermSize is in `term::test` module | Low | Low | It is a public struct usable in production code; alternatively implement the Dimensions trait on a custom struct |
| vte Color enum variants differ from arcterm-core Color | Low | Medium | Fetch exact vte 0.15 Color enum source to verify; update ansi_color_to_glyphon accordingly |
| alacritty_terminal license is Apache-2.0 vs arcterm MIT | Low | Low | Apache-2.0 is compatible with MIT for binary distribution; note in LICENSES/ |
| EventLoop uses polling (not tokio); synchronization with tokio main loop | Medium | Medium | Use `Arc<Mutex<VecDeque<Event>>>` or `std::sync::mpsc` channel; post `EventLoopProxy::send_event` when wakeup arrives |
| `display_iter` iterator replaces 2D slice API; renderer rewrite required | Certain | Medium | The renderer's `rows_for_viewport()` and `shape_row_into_buffer` must be rewritten; scope is well-defined |
| OSC 133 (shell integration A/B/C/D) handling | Medium | Medium | Alacritty does not implement OSC 133; pre-filter must also intercept OSC 133 sequences |
| alacritty API breaking changes in future versions | Low | Low | Decision D1 accepts this; no fork; adapt on upgrade |

---

## Recommended Approach

### Architecture (maps to D1–D5)

**PTY + EventLoop pattern (D2):**

```
shell process
    |
  Pty (master fd)
    |
[PreFilter task: reads raw fd, strips OSC 7770 + APC, dispatches to channels]
    |
  pipe (write end → read end)
    |
EventLoop (owns pipe read-end as Pty via tty::from_fd)
    |
  Arc<FairMutex<Term<ArcTermEventListener>>>
    |
[Main winit thread: locks Term on Wakeup, calls renderable_content()]
```

The `PreFilter` runs as a tokio task (or raw thread) that reads from `pty.file()` directly (before EventLoop takes it). It extracts and dispatches:
- APC sequences → `image_tx: mpsc::Sender<Vec<u8>>` (Kitty payloads)
- OSC 7770 sequences → `osc7770_tx: mpsc::Sender<Osc7770Event>` (structured content, tool calls, etc.)
- OSC 133 sequences → `osc133_tx: mpsc::Sender<Osc133Event>` (exit codes, command tracking)
- Everything else → pipe write-end → EventLoop → Term

**Alternative (bypass EventLoop):** Skip EventLoop entirely; read from PTY fd in a tokio task, pre-filter, then call `vte_parser.advance(&mut *term.lock(), clean_bytes)` directly. This is simpler architecturally (no pipe) but requires re-implementing EventLoop's write path (`Msg::Input` → `pty.writer().write()`). Given that EventLoop's write path is non-trivial (handles `WouldBlock`, maintains a `VecDeque<Cow<[u8]>>`), the pipe approach is recommended to preserve EventLoop's battle-tested I/O.

### API Mappings Summary

| Old arcterm code | New alacritty_terminal equivalent |
|------------------|----------------------------------|
| `arcterm_core::Grid` | `alacritty_terminal::Grid<Cell>` |
| `arcterm_core::Cell` + `CellAttrs` | `alacritty_terminal::term::cell::Cell` |
| `arcterm_core::Color` | `vte::ansi::Color` |
| `arcterm_core::CursorPos` | `alacritty_terminal::index::Point` |
| `arcterm_core::GridSize` | `alacritty_terminal::event::WindowSize` (for PTY) or `TermSize` (for Term) |
| `arcterm_vt::ApcScanner::advance` | New `PreFilter::advance` (handles both APC and OSC 7770/133) |
| `arcterm_pty::PtySession::new` | `tty::new(&options, window_size, window_id)` |
| `arcterm_pty::PtySession::write` | `EventLoopSender::send(Msg::Input(...))` |
| `arcterm_pty::PtySession::resize` | `EventLoopSender::send(Msg::Resize(...))` |
| `arcterm_pty::PtySession::child_pid` | `pty.child().id()` (before EventLoop takes ownership) |
| `terminal.process_pty_output(bytes)` | Removed; EventLoop drives parser |
| `terminal.take_pending_replies()` | `Event::PtyWrite(s)` in EventListener impl |
| `terminal.grid()` | `arc_term.lock_unfair().grid()` |
| `grid.rows_for_viewport()` | `term.renderable_content().display_iter` |
| `grid.cursor.row / .col` | `content.cursor.point.line.0 / .column.0` |
| `grid.modes.cursor_visible` | `content.mode.contains(TermMode::SHOW_CURSOR)` |
| `set_scroll_offset(n)` | `term.scroll_display(Scroll::Delta(n as i32))` |

---

## Implementation Considerations

### Renderer changes (arcterm-render)

- `PaneRenderInfo.grid` type changes from `&arcterm_core::Grid` to an opaque data structure (a snapshot of `RenderableContent` or a `Vec<RenderableRow>` pre-extracted by the app layer before the lock is released). Holding the `FairMutex` guard across a full frame render is inadvisable.
- The recommended pattern: lock `Term`, call `renderable_content()`, collect cells into a `Vec<IndexedCell>` snapshot, unlock, then render from the snapshot. This requires a new intermediate type in arcterm-render or arcterm-app.
- `build_quad_instances_at` and `shape_row_into_buffer` both take row slices; they must be adapted for the flat iterator model.

### Testing strategy (D3)

Delete all unit tests in `arcterm-core`, `arcterm-vt`, `arcterm-pty`. Write integration tests in `arcterm-app/tests/` or a new `arcterm-integration` crate that:
1. Spawn a PTY shell, feed known escape sequences, read grid state
2. Verify OSC 7770 pre-filter correctly extracts and dispatches sequences
3. Verify Kitty APC pre-filter correctly extracts payloads
4. Verify resize propagates to PTY and Term correctly

### Migration path

The crates can be removed in dependency order: `arcterm-pty` → `arcterm-vt` → `arcterm-core`. Each removal breaks the build; Phase 12 plans should be structured as a single atomic replacement (all three crates replaced in one wave).

---

## Sources

1. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/ — crate top-level docs
2. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/term/struct.Term.html — Term struct
3. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/grid/struct.Grid.html — Grid struct
4. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/term/cell/struct.Cell.html — Cell struct
5. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/term/cell/struct.Flags.html — Cell Flags
6. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/term/struct.RenderableContent.html — RenderableContent
7. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/term/struct.RenderableCursor.html — RenderableCursor
8. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/term/struct.Config.html — Term Config
9. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/term/struct.TermMode.html — TermMode bitflags
10. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/term/test/struct.TermSize.html — TermSize
11. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/tty/index.html — tty module
12. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/tty/fn.new.html — tty::new
13. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/tty/struct.Pty.html — Pty struct
14. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/tty/struct.Options.html — Pty Options
15. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/event_loop/struct.EventLoop.html — EventLoop
16. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/event_loop/enum.Msg.html — EventLoop Msg
17. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/event_loop/struct.EventLoopSender.html — EventLoopSender
18. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/event/trait.EventListener.html — EventListener
19. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/event/enum.Event.html — Event enum
20. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/event/struct.WindowSize.html — WindowSize
21. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/grid/struct.GridIterator.html — GridIterator
22. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/grid/trait.Dimensions.html — Dimensions trait
23. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/index/struct.Point.html — Point
24. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/sync/struct.FairMutex.html — FairMutex
25. https://docs.rs/alacritty_terminal/0.25.1/alacritty_terminal/term/color/struct.Colors.html — Colors
26. https://github.com/alacritty/alacritty/blob/master/alacritty_terminal/src/event.rs — Event source
27. https://raw.githubusercontent.com/alacritty/alacritty/master/alacritty_terminal/src/tty/unix.rs — PTY Unix impl
28. https://raw.githubusercontent.com/alacritty/alacritty/master/alacritty_terminal/Cargo.toml — dependency manifest
29. https://crates.io/api/v1/crates/alacritty_terminal — download stats, license
30. Code analysis: `/Users/lgbarn/Personal/arcterm/arcterm-vt/src/processor.rs`
31. Code analysis: `/Users/lgbarn/Personal/arcterm/arcterm-app/src/terminal.rs`
32. Code analysis: `/Users/lgbarn/Personal/arcterm/arcterm-render/src/text.rs`
33. Code analysis: `/Users/lgbarn/Personal/arcterm/arcterm-render/src/renderer.rs`
34. Code analysis: `/Users/lgbarn/Personal/arcterm/arcterm-core/src/cell.rs`
35. Code analysis: `/Users/lgbarn/Personal/arcterm/arcterm-core/src/grid.rs`

---

## Uncertainty Flags

1. **vte::ansi::Color exact variant names:** The docs.rs page for `vte::ansi::Color` returned 404. The cell documentation confirms `fg: Color` and `bg: Color` use the vte Color type, and that it is from `vte` 0.15.0. The arcterm workspace already uses `vte = "0.15"`, so the type is available, but the exact variant names (e.g., `Color::Named(NamedColor)` vs `Color::Indexed(u8)` vs `Color::Spec(Rgb)`) must be confirmed against the vte 0.15 source before `ansi_color_to_glyphon` is rewritten.

2. **`tty::from_fd` signature and platform support:** The pipe-based pre-filter approach relies on `tty::from_fd`. The function exists in the tty module but its full signature and whether it works on all platforms (macOS, Linux) was not confirmed. If `from_fd` is Unix-only or has restrictions, the bypass-EventLoop approach may be necessary.

3. **Exact `renderable_content()` iteration order and line numbering:** The `display_iter` documentation states it iterates "all visible cells" but does not confirm whether the iteration is row-major (left-to-right, top-to-bottom) or whether `Line(0)` is always the top visible row. The renderer depends on row ordering. This should be verified with a minimal test before the renderer is rewritten.

4. **`TermSize` in `term::test` module for production use:** While the struct is public, its placement in a `test` module is unusual. It may be marked `#[doc(hidden)]` in a future release. The safe alternative is to implement the `Dimensions` trait on a custom `ArcTermSize` struct, which is trivial and eliminates the dependency on a test helper.

5. **OSC 133 interception:** The current arcterm design processes OSC 133 (A/B/C/D — shell integration) in `arcterm-vt`. Alacritty does not implement OSC 133. This research confirms the pre-filter must handle it, but the exact OSC 133 sequence format used by arcterm's shell integration was not re-verified in this research pass. The Phase 12 plan should confirm which OSC 133 sub-commands arcterm currently handles.

6. **EventLoop `Msg::Resize` variant:** The documentation listed `Msg` variants as `Input`, `Shutdown`, and `Resize`. The `Resize` variant type was not confirmed (it likely wraps `WindowSize`). The sender pattern `loop_sender.send(Msg::Resize(window_size))` should be verified against the actual enum definition before implementation.
