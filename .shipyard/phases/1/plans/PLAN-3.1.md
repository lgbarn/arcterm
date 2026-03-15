---
phase: foundation
plan: "3.1"
wave: 3
dependencies: ["2.1", "2.2", "2.3"]
must_haves:
  - arcterm-app binary opens a window running an interactive shell
  - Keyboard input is sent to the PTY and output is displayed
  - VT100 sequences render correctly (colors, cursor movement, erase)
  - ls, vim, top produce usable output
  - Cursor is visible and positioned correctly
files_touched:
  - arcterm-app/src/main.rs
  - arcterm-app/src/terminal.rs
  - arcterm-app/src/input.rs
tdd: false
---

# Plan 3.1 -- Application Shell: PTY-VT-Renderer Integration

**Wave 3** | Depends on: Plans 2.1, 2.2, 2.3 | Parallel with: Plan 3.2

## Goal

Wire together PtySession, Processor, Grid, and Renderer into the arcterm-app binary. After this plan, running `cargo run --package arcterm-app` opens a GPU-rendered terminal window running the user's shell, where typing characters produces visible output and programs like `ls` and `vim` work.

---

<task id="1" files="arcterm-app/src/main.rs, arcterm-app/src/terminal.rs" tdd="false">
  <action>
    Implement the main application wiring in `arcterm-app`.

    **`main.rs`:**
    ```rust
    fn main() {
        env_logger::init();
        // Create tokio runtime manually (event loop must run on main thread)
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();

        let event_loop = winit::event_loop::EventLoop::new().unwrap();
        let proxy = event_loop.create_proxy();
        let mut app = App::new(rt.handle().clone(), proxy);
        event_loop.run_app(&mut app).unwrap();
    }
    ```

    **`terminal.rs` -- `Terminal` struct:**
    Owns the PTY session, VT processor, and grid. Orchestrates the data flow.

    ```rust
    pub struct Terminal {
        pty: PtySession,
        processor: Processor,
        grid: Grid,
    }
    ```

    **`Terminal::new(size: GridSize) -> Result<Self>`:**
    Create PtySession with size, Processor, and Grid with size.

    **`Terminal::process_pty_output(&mut self, bytes: &[u8])`:**
    Call `self.processor.advance(&mut self.grid, bytes)`.

    **`Terminal::write_input(&mut self, data: &[u8])`:**
    Call `self.pty.write(data)`.

    **`Terminal::grid(&self) -> &Grid`:**
    Return reference to grid for rendering.

    **`Terminal::resize(&mut self, size: GridSize)`:**
    Resize grid and PTY.

    **`App` struct (implements `ApplicationHandler`):**
    ```rust
    struct App {
        rt_handle: tokio::runtime::Handle,
        proxy: EventLoopProxy<()>,
        window: Option<Arc<Window>>,
        renderer: Option<Renderer>,
        terminal: Option<Terminal>,
        pty_rx: Option<tokio::sync::mpsc::Receiver<Vec<u8>>>,
    }
    ```

    **`App::resumed()`:**
    1. Create window (title "Arcterm", 1024x768).
    2. Create Renderer from window.
    3. Calculate GridSize from window dimensions and cell size.
    4. Create Terminal with calculated GridSize.
    5. Set up PTY output forwarding: the Terminal's PtySession already has an output channel. Store the receiver on App. Spawn a tokio task that polls the receiver and calls `proxy.wake_up()` whenever data arrives. The actual data stays in the channel; `wake_up()` just triggers a winit event loop iteration.

    Wait -- the PtySession owns the receiver. Restructure: have PtySession return the receiver on construction, or have Terminal expose it. The cleanest approach:
    - `PtySession::new()` returns `(PtySession, mpsc::Receiver<Vec<u8>>)`.
    - `Terminal::new()` returns `(Terminal, mpsc::Receiver<Vec<u8>>)`.
    - App stores the receiver and drains it in the event loop.

    **`App::window_event()` handling:**
    - `RedrawRequested`:
      1. Drain PTY output: `while let Ok(bytes) = self.pty_rx.try_recv() { terminal.process_pty_output(&bytes); }`
      2. Call `renderer.render_frame(terminal.grid(), scale_factor)`.
      3. Handle `SurfaceError::Lost` by resizing.
    - `Resized(size)`:
      1. `renderer.resize(size.width, size.height)`.
      2. Calculate new GridSize, call `terminal.resize(new_size)`.
    - `CloseRequested`: exit event loop.
    - `KeyboardInput { event, .. }`: see Task 2.

    **`App::about_to_wait()`:**
    Drain PTY channel. If any data was received, call `window.request_redraw()`. This creates the event-driven rendering loop: redraw only when there is new output or input.

    Also set up a cursor blink timer or just request_redraw unconditionally for Phase 1 simplicity (continuous rendering at vsync rate when idle is acceptable for Phase 1).
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-app 2>&1 | tail -5</verify>
  <done>`cargo build --package arcterm-app` succeeds. Running `cargo run --package arcterm-app` opens a window. The shell prompt appears rendered in the window. Shell output from the initial profile/rc scripts is visible.</done>
</task>

<task id="2" files="arcterm-app/src/input.rs" tdd="false">
  <action>
    Implement keyboard input translation in `arcterm-app/src/input.rs`.

    **`translate_key_event(event: &winit::event::KeyEvent) -> Option<Vec<u8>>`:**

    Map winit key events to the byte sequences that a terminal expects.

    **Priority 1 -- use `event.text`:**
    If `event.state == ElementState::Pressed` and `event.text.is_some()`, return the text bytes directly. This handles all printable characters, shift combinations, and IME input correctly.

    **Priority 2 -- special keys (when text is None):**
    Match on `event.logical_key`:
    - `Key::Named(NamedKey::Enter)` -> `b"\r"` (CR, not LF -- terminals send CR on Enter)
    - `Key::Named(NamedKey::Backspace)` -> `b"\x7f"` (DEL, not BS -- modern terminals)
    - `Key::Named(NamedKey::Tab)` -> `b"\t"`
    - `Key::Named(NamedKey::Escape)` -> `b"\x1b"`
    - `Key::Named(NamedKey::ArrowUp)` -> `b"\x1b[A"`
    - `Key::Named(NamedKey::ArrowDown)` -> `b"\x1b[B"`
    - `Key::Named(NamedKey::ArrowRight)` -> `b"\x1b[C"`
    - `Key::Named(NamedKey::ArrowLeft)` -> `b"\x1b[D"`
    - `Key::Named(NamedKey::Home)` -> `b"\x1b[H"`
    - `Key::Named(NamedKey::End)` -> `b"\x1b[F"`
    - `Key::Named(NamedKey::PageUp)` -> `b"\x1b[5~"`
    - `Key::Named(NamedKey::PageDown)` -> `b"\x1b[6~"`
    - `Key::Named(NamedKey::Delete)` -> `b"\x1b[3~"`
    - `Key::Named(NamedKey::F1)` through `F12` -> standard VT220 F-key sequences

    **Priority 3 -- Ctrl combinations:**
    If Ctrl modifier is active and `event.logical_key` is `Key::Character(c)`:
    - For c in 'a'..='z': send byte `c as u8 - 0x60` (Ctrl+A = 0x01, Ctrl+C = 0x03, etc.)
    - Ctrl+[ = ESC (0x1b), Ctrl+\\ = 0x1c, Ctrl+] = 0x1d

    **Integration in App::window_event:**
    On `WindowEvent::KeyboardInput { event, .. }` where `event.state == ElementState::Pressed`:
    1. Call `translate_key_event(&event)`.
    2. If Some(bytes), call `terminal.write_input(&bytes)`.
    3. Call `window.request_redraw()` to immediately show the echo.

    **Cursor rendering:**
    Add a simple block cursor to the renderer. In `Renderer::render_frame`, after rendering text, draw a filled rectangle at the cursor position using wgpu. For Phase 1, the simplest approach is to render the cursor as an inverted cell: when preparing the grid for glyphon, if a cell is at the cursor position, swap its fg and bg colors. This avoids needing a separate rectangle-drawing pipeline.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-app 2>&1 | tail -5</verify>
  <done>`cargo build --package arcterm-app` succeeds. Running the binary: typing characters shows them in the terminal. Enter key sends commands. Arrow keys move the cursor in shells that support it (bash/zsh with readline). Ctrl+C sends SIGINT. Ctrl+D sends EOF. Backspace deletes characters. A block cursor is visible at the current cursor position.</done>
</task>

<task id="3" files="arcterm-app/src/main.rs" tdd="false">
  <action>
    Integration testing and performance verification.

    **Manual test checklist (document in a comment block at top of main.rs):**
    1. `ls --color` -- files display with ANSI colors.
    2. `vim` -- opens, shows status bar, typing works, `:q` exits cleanly.
    3. `top` -- displays process list, updates periodically, `q` exits.
    4. `htop` -- displays with 256-color bars, navigable with arrow keys.
    5. Window resize -- grid resizes, prompt re-renders at correct width.
    6. Ctrl+C interrupts a `sleep 100` command.
    7. `echo -e "\x1b[31mred \x1b[32mgreen \x1b[0mnormal"` -- colors render correctly.
    8. Rapid output (`yes | head -1000`) does not crash or hang.

    **Latency measurement:**
    Add a compile-time feature flag `latency-trace` that, when enabled, logs timestamps at:
    - Key event received (in window_event handler)
    - PTY write completed
    - PTY output received (in about_to_wait drain)
    - Render frame submitted

    Use `std::time::Instant` for timestamps. Log with `log::debug!`. This allows measuring key-to-screen latency with `RUST_LOG=debug cargo run --features latency-trace`.

    **Cold start measurement:**
    Log `Instant::now()` at the very first line of `main()`. Log again when the first frame is presented. The difference is cold start time. Print this as an info log: "Cold start: {elapsed}ms".

    **Error handling polish:**
    - If PtySession creation fails, log the error and exit with a meaningful message (not a panic).
    - If the shell exits (PTY output channel closes), display "Shell exited" in the window and wait for the user to close the window (or exit after a keypress).
    - Handle wgpu SurfaceError::Outdated by reconfiguring the surface (can happen on macOS window system events).
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-app 2>&1 | tail -5</verify>
  <done>`cargo build --package arcterm-app` succeeds with and without `--features latency-trace`. The binary opens a working terminal. `ls --color`, `vim`, and `top` produce usable output. Cold start time is logged. Shell exit is handled gracefully without panic.</done>
</task>
