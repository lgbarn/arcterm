//! arcterm-app — application entry point.
//!
//! # Manual Test Checklist (Task 3 acceptance criteria)
//!
//! Run `cargo run --package arcterm-app` and verify each item:
//!
//! 1. `ls --color` — coloured directory listing renders correctly with ANSI colours.
//! 2. `vim` — full-screen editor launches, redraws on resize, and exits cleanly.
//! 3. `top` — live updating display renders without corruption.
//! 4. `htop` — same as top; mouse input is not required for pass.
//! 5. Window resize — drag the window edge; the shell prompt reflows to the new width.
//! 6. Ctrl+C — sends SIGINT; running process terminates and returns to shell prompt.
//! 7. `echo -e "\033[31mred\033[0m"` — red text appears, then colour resets.
//! 8. Rapid output (`cat /dev/urandom | head -c 1M | base64`) — no hang or crash.

mod input;
mod selection;
mod terminal;

use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(feature = "latency-trace")]
use std::time::Instant as TraceInstant;

use arcterm_core::{Cell, CellAttrs, Color, CursorPos};
use arcterm_render::Renderer;
use selection::{generate_selection_quads, pixel_to_cell, Clipboard, Selection, SelectionMode, SelectionQuad};
use terminal::Terminal;
use tokio::sync::mpsc;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::ModifiersState,
    window::{Window, WindowId},
};

/// Maximum gap between clicks to be counted as a multi-click (in ms).
const MULTI_CLICK_INTERVAL_MS: u64 = 400;

/// Lines scrolled per mouse-wheel tick.
const SCROLL_LINES_PER_TICK: usize = 3;

fn main() {
    env_logger::init();

    #[cfg(feature = "latency-trace")]
    let cold_start = TraceInstant::now();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    let event_loop = EventLoop::new().unwrap();
    let mut app = App {
        state: None,
        modifiers: ModifiersState::empty(),
        #[cfg(feature = "latency-trace")]
        cold_start,
    };
    event_loop.run_app(&mut app).unwrap();
}

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

struct AppState {
    window: Arc<Window>,
    renderer: Renderer,
    terminal: Terminal,
    pty_rx: mpsc::Receiver<Vec<u8>>,
    /// Set to `true` once the PTY channel closes (shell has exited).
    shell_exited: bool,

    // ---- selection & clipboard ----
    selection: Selection,
    /// Clipboard instance; `None` if the system clipboard is unavailable.
    clipboard: Option<Clipboard>,
    /// Last known physical cursor position (pixels).
    last_cursor_position: (f64, f64),
    /// Timestamp of the last left-button press (for multi-click detection).
    last_click_time: Option<Instant>,
    /// Consecutive click count: 1 = single, 2 = double, 3 = triple.
    click_count: u32,

    // ---- selection rendering ----
    /// Pre-computed quads for the current selection.
    /// Updated every `RedrawRequested`. Stored here for future quad-pipeline
    /// integration — not yet submitted to the GPU.
    selection_quads: Vec<SelectionQuad>,
}

struct App {
    state: Option<AppState>,
    /// Current keyboard modifier state, updated by ModifiersChanged events.
    modifiers: ModifiersState,
    #[cfg(feature = "latency-trace")]
    cold_start: TraceInstant,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let window_attrs = Window::default_attributes()
            .with_inner_size(LogicalSize::new(1024u32, 768u32))
            .with_title("Arcterm");

        let window = Arc::new(
            event_loop
                .create_window(window_attrs)
                .expect("failed to create window"),
        );

        let renderer = Renderer::new(window.clone());

        let size = renderer.grid_size_for_window(
            window.inner_size().width,
            window.inner_size().height,
            window.scale_factor(),
        );

        let (terminal, pty_rx) = Terminal::new(size).unwrap_or_else(|e| {
            log::error!("Failed to create PTY session: {e}");
            std::process::exit(1);
        });

        let clipboard = Clipboard::new()
            .map_err(|e| log::warn!("Clipboard unavailable: {e}"))
            .ok();

        self.state = Some(AppState {
            window,
            renderer,
            terminal,
            pty_rx,
            shell_exited: false,
            selection: Selection::default(),
            clipboard,
            last_cursor_position: (0.0, 0.0),
            last_click_time: None,
            click_count: 0,
            selection_quads: Vec::new(),
        });
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let Some(state) = &mut self.state else {
            return;
        };

        if state.shell_exited {
            return;
        }

        let mut got_data = false;
        loop {
            match state.pty_rx.try_recv() {
                Ok(bytes) => {
                    #[cfg(feature = "latency-trace")]
                    let t0 = TraceInstant::now();

                    state.terminal.process_pty_output(&bytes);
                    got_data = true;

                    #[cfg(feature = "latency-trace")]
                    log::debug!(
                        "[latency] PTY output processed: {} bytes in {:?}",
                        bytes.len(),
                        t0.elapsed()
                    );
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    log::info!("PTY channel closed — shell has exited");
                    state.shell_exited = true;
                    state.window.request_redraw();
                    break;
                }
            }
        }

        if got_data {
            // New PTY output: if we're scrolled back into history, clear any
            // active selection (content has shifted) and reset to the live view.
            let grid = state.terminal.grid_mut();
            if grid.scroll_offset > 0 {
                state.selection.clear();
                grid.scroll_offset = 0;
            }
            state.window.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = &mut self.state else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
            }

            WindowEvent::Resized(size) => {
                if size.width == 0 || size.height == 0 {
                    return;
                }
                state.renderer.resize(size.width, size.height);
                let new_grid_size = state.renderer.grid_size_for_window(
                    size.width,
                    size.height,
                    state.window.scale_factor(),
                );
                state.terminal.resize(new_grid_size);
                state.window.request_redraw();
            }

            // -----------------------------------------------------------------
            // Mouse cursor movement — extend an in-progress drag selection.
            // -----------------------------------------------------------------
            WindowEvent::CursorMoved { position, .. } => {
                state.last_cursor_position = (position.x, position.y);
                // Only extend selection when left button is being held down.
                // We detect this by checking if mode != None (set on press,
                // cleared on release).
                if state.selection.mode != SelectionMode::None {
                    let cell = cursor_to_cell(state, position.x, position.y);
                    state.selection.update(cell);
                    state.window.request_redraw();
                }
            }

            // -----------------------------------------------------------------
            // Mouse button press/release — start/stop selection, copy on triple.
            // -----------------------------------------------------------------
            WindowEvent::MouseInput { state: btn_state, button, .. } => {
                if button == MouseButton::Left {
                    match btn_state {
                        ElementState::Pressed => {
                            let now = Instant::now();
                            let multi = state
                                .last_click_time
                                .map(|prev| {
                                    now.duration_since(prev) < Duration::from_millis(MULTI_CLICK_INTERVAL_MS)
                                })
                                .unwrap_or(false);

                            if multi {
                                state.click_count = (state.click_count + 1).min(3);
                            } else {
                                state.click_count = 1;
                            }
                            state.last_click_time = Some(now);

                            let (px, py) = state.last_cursor_position;
                            let cell = cursor_to_cell(state, px, py);
                            let mode = match state.click_count {
                                1 => SelectionMode::Character,
                                2 => SelectionMode::Word,
                                _ => SelectionMode::Line,
                            };
                            state.selection.start(cell, mode);
                            state.window.request_redraw();
                        }
                        ElementState::Released => {
                            // Keep the selection visible but stop extending it.
                            // Mode remains set so renders show the highlight.
                        }
                    }
                }
            }

            // -----------------------------------------------------------------
            // Mouse wheel — scroll the viewport.
            // -----------------------------------------------------------------
            WindowEvent::MouseWheel { delta, .. } => {
                let lines: i32 = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as i32,
                    MouseScrollDelta::PixelDelta(pos) => {
                        // Convert pixel delta to approximate line count.
                        let cell_h = state.renderer.text.cell_size.height as f64;
                        (pos.y / cell_h).round() as i32
                    }
                };

                if lines != 0 {
                    let grid = state.terminal.grid_mut();
                    let max_offset = grid.scrollback.len();
                    let current = grid.scroll_offset as i32;
                    let new_offset = (current - lines * SCROLL_LINES_PER_TICK as i32)
                        .clamp(0, max_offset as i32) as usize;
                    grid.scroll_offset = new_offset;
                    state.window.request_redraw();
                }
            }

            // -----------------------------------------------------------------
            // Redraw — render frame and recompute selection quads.
            // -----------------------------------------------------------------
            WindowEvent::RedrawRequested => {
                #[cfg(feature = "latency-trace")]
                let t0 = TraceInstant::now();

                // Recompute selection quads for the current frame dimensions.
                {
                    let grid = state.terminal.grid();
                    let cell_w = state.renderer.text.cell_size.width;
                    let cell_h = state.renderer.text.cell_size.height;
                    let scale = state.window.scale_factor() as f32;
                    state.selection_quads = generate_selection_quads(
                        &state.selection,
                        grid.size.rows,
                        grid.size.cols,
                        cell_w,
                        cell_h,
                        scale,
                    );
                    log::trace!(
                        "selection quads: {} rect(s) for mode {:?}",
                        state.selection_quads.len(),
                        state.selection.mode
                    );
                }

                if state.shell_exited {
                    let mut display = state.terminal.grid().clone();
                    let last_row = display.size.rows.saturating_sub(1);
                    let msg = "[ Shell exited — press any key to close ]";
                    let banner_attrs = CellAttrs {
                        fg: Color::Indexed(11),
                        bg: Color::Indexed(0),
                        bold: true,
                        ..CellAttrs::default()
                    };
                    if let Some(row) = display.cells.get_mut(last_row) {
                        for cell in row.iter_mut() {
                            *cell = Cell {
                                c: ' ',
                                attrs: banner_attrs,
                                dirty: true,
                            };
                        }
                        for (col, ch) in msg.chars().enumerate() {
                            if col >= display.size.cols {
                                break;
                            }
                            row[col] = Cell {
                                c: ch,
                                attrs: banner_attrs,
                                dirty: true,
                            };
                        }
                    }
                    display.cursor = CursorPos {
                        row: last_row.saturating_sub(1),
                        col: 0,
                    };
                    state
                        .renderer
                        .render_frame(&display, state.window.scale_factor());
                } else {
                    state
                        .renderer
                        .render_frame(state.terminal.grid(), state.window.scale_factor());
                }

                #[cfg(feature = "latency-trace")]
                {
                    log::debug!("[latency] frame submitted in {:?}", t0.elapsed());
                    static FIRST_FRAME: std::sync::atomic::AtomicBool =
                        std::sync::atomic::AtomicBool::new(true);
                    if FIRST_FRAME.swap(false, std::sync::atomic::Ordering::Relaxed) {
                        log::info!(
                            "[latency] cold start → first frame: {:?}",
                            self.cold_start.elapsed()
                        );
                    }
                }
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    #[cfg(feature = "latency-trace")]
                    let t0 = TraceInstant::now();

                    // macOS: super key is the Command key.
                    let super_key = self.modifiers.super_key();

                    // Cmd+C — copy selection to clipboard.
                    if super_key {
                        use winit::keyboard::Key;
                        if let Key::Character(ref s) = event.logical_key {
                            match s.as_str() {
                                "c" | "C" => {
                                    let text = state
                                        .selection
                                        .extract_text(state.terminal.grid());
                                    if !text.is_empty() {
                                        if let Some(cb) = &mut state.clipboard {
                                            if let Err(e) = cb.copy(&text) {
                                                log::warn!("Clipboard copy failed: {e}");
                                            }
                                        }
                                    }
                                    return;
                                }
                                // Cmd+V — paste from clipboard.
                                "v" | "V" => {
                                    if let Some(cb) = &mut state.clipboard {
                                        match cb.paste() {
                                            Ok(text) => {
                                                let bracketed =
                                                    state.terminal.grid().modes.bracketed_paste;
                                                if bracketed {
                                                    let mut payload =
                                                        b"\x1b[200~".to_vec();
                                                    payload.extend_from_slice(
                                                        text.as_bytes(),
                                                    );
                                                    payload.extend_from_slice(b"\x1b[201~");
                                                    state.terminal.write_input(&payload);
                                                } else {
                                                    state
                                                        .terminal
                                                        .write_input(text.as_bytes());
                                                }
                                                state.window.request_redraw();
                                            }
                                            Err(e) => {
                                                log::warn!("Clipboard paste failed: {e}");
                                            }
                                        }
                                    }
                                    return;
                                }
                                _ => {}
                            }
                        }
                    }

                    if let Some(bytes) = input::translate_key_event(&event, self.modifiers) {
                        #[cfg(feature = "latency-trace")]
                        log::debug!(
                            "[latency] key → PTY write ({} bytes) after {:?}",
                            bytes.len(),
                            t0.elapsed()
                        );

                        state.terminal.write_input(&bytes);
                        state.window.request_redraw();
                    }
                }
            }

            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: convert a physical pixel position to a grid CellPos.
// ---------------------------------------------------------------------------

fn cursor_to_cell(state: &AppState, px: f64, py: f64) -> selection::CellPos {
    let scale = state.window.scale_factor();
    let cell_w = state.renderer.text.cell_size.width as f64;
    let cell_h = state.renderer.text.cell_size.height as f64;
    pixel_to_cell(px, py, cell_w, cell_h, scale)
}
