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
mod terminal;

use std::sync::Arc;

#[cfg(feature = "latency-trace")]
use std::time::Instant;

use arcterm_core::{Cell, CellAttrs, Color, CursorPos};
use arcterm_render::Renderer;
use terminal::Terminal;
use tokio::sync::mpsc;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::ModifiersState,
    window::{Window, WindowId},
};

fn main() {
    env_logger::init();

    #[cfg(feature = "latency-trace")]
    let cold_start = Instant::now();

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
    /// Used to display an in-window indicator and skip the polling loop.
    shell_exited: bool,
}

struct App {
    state: Option<AppState>,
    /// Current keyboard modifier state, updated by ModifiersChanged events.
    modifiers: ModifiersState,
    #[cfg(feature = "latency-trace")]
    cold_start: Instant,
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

        self.state = Some(AppState {
            window,
            renderer,
            terminal,
            pty_rx,
            shell_exited: false,
        });
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let Some(state) = &mut self.state else {
            return;
        };

        // Skip the channel loop once the shell has exited — the receiver
        // would return Disconnected on every tick otherwise.
        if state.shell_exited {
            return;
        }

        let mut got_data = false;
        loop {
            match state.pty_rx.try_recv() {
                Ok(bytes) => {
                    #[cfg(feature = "latency-trace")]
                    let t0 = Instant::now();

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
                    // Shell has exited — set flag, request one last redraw to
                    // display the "Shell exited" indicator, then stop polling.
                    log::info!("PTY channel closed — shell has exited");
                    state.shell_exited = true;
                    state.window.request_redraw();
                    break;
                }
            }
        }

        if got_data {
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

            WindowEvent::RedrawRequested => {
                #[cfg(feature = "latency-trace")]
                let t0 = Instant::now();

                if state.shell_exited {
                    // Render a clone of the grid with the last row replaced by a
                    // "Shell exited" banner so the user knows the window is inert.
                    let mut display = state.terminal.grid().clone();
                    let last_row = display.size.rows.saturating_sub(1);
                    let msg = "[ Shell exited — press any key to close ]";
                    let banner_attrs = CellAttrs {
                        fg: Color::Indexed(11), // bright yellow
                        bg: Color::Indexed(0),  // black
                        bold: true,
                        ..CellAttrs::default()
                    };
                    // Clear the last row and write the banner.
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
                    // Park the cursor off the last row so it does not overlay the banner.
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

                    // Log cold-start time on the very first frame.
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
                    let t0 = Instant::now();

                    if let Some(bytes) = input::translate_key_event(&event, self.modifiers) {
                        #[cfg(feature = "latency-trace")]
                        log::debug!(
                            "[latency] key → PTY write ({} bytes) after {:?}",
                            bytes.len(),
                            t0.elapsed()
                        );

                        state.terminal.write_input(&bytes);
                        // Immediately trigger a redraw so typed characters show
                        // up even when the shell has not yet echoed them back.
                        state.window.request_redraw();
                    }
                }
            }

            _ => {}
        }
    }
}
