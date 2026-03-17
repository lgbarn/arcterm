//! Window example: renders a test snapshot using arcterm-render.
//!
//! Run with: cargo run --package arcterm-render --example window

use std::sync::Arc;

use arcterm_render::{RenderSnapshot, Renderer, SnapshotCell, SnapshotColor};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop
        .run_app(&mut App { state: None })
        .expect("event loop error");
}

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

struct AppState {
    window: Arc<Window>,
    renderer: Renderer,
    snapshot: RenderSnapshot,
}

struct App {
    state: Option<AppState>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let window_attrs = Window::default_attributes()
            .with_inner_size(LogicalSize::new(1024u32, 768u32))
            .with_title("Arcterm — render example");
        let window = Arc::new(
            event_loop
                .create_window(window_attrs)
                .expect("failed to create window"),
        );

        let renderer = match Renderer::new(window.clone(), 14.0) {
            Ok(r) => r,
            Err(e) => {
                log::error!("GPU initialization failed: {e}");
                event_loop.exit();
                return;
            }
        };

        // Build a test snapshot with demo content.
        let (rows, cols) = renderer.grid_size_for_window(
            window.inner_size().width,
            window.inner_size().height,
            window.scale_factor(),
        );
        let snapshot = build_test_snapshot(rows, cols);

        self.state = Some(AppState {
            window,
            renderer,
            snapshot,
        });
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

            WindowEvent::Resized(size) => {
                state.renderer.resize(size.width, size.height);
                let (rows, cols) = state.renderer.grid_size_for_window(
                    size.width,
                    size.height,
                    state.window.scale_factor(),
                );
                state.snapshot = build_test_snapshot(rows, cols);
                state.window.request_redraw();
            }

            WindowEvent::RedrawRequested => {
                state
                    .renderer
                    .render_frame(&state.snapshot, state.window.scale_factor());
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Request a redraw every time the event loop is about to wait, so the
        // window renders continuously rather than only on reactive events.
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }
}

// ---------------------------------------------------------------------------
// Test snapshot construction
// ---------------------------------------------------------------------------

/// Build a `RenderSnapshot` filled with demo content.
fn build_test_snapshot(rows: usize, cols: usize) -> RenderSnapshot {
    let mut cells: Vec<SnapshotCell> = vec![SnapshotCell::default(); rows * cols];

    // Row 0: "Hello, Arcterm!" in default colors.
    write_row(
        &mut cells,
        0,
        cols,
        "Hello, Arcterm!",
        SnapshotColor::Default,
        false,
    );

    // Row 1: bright red label.
    write_row(
        &mut cells,
        1,
        cols,
        "  Red text row",
        SnapshotColor::Indexed(9),
        false,
    );

    // Row 2: bright green label.
    write_row(
        &mut cells,
        2,
        cols,
        "  Green text row",
        SnapshotColor::Indexed(10),
        false,
    );

    // Row 3: bright cyan label.
    write_row(
        &mut cells,
        3,
        cols,
        "  Cyan text row",
        SnapshotColor::Indexed(14),
        false,
    );

    // Row 4: true-color RGB example (orange).
    write_row(
        &mut cells,
        4,
        cols,
        "  RGB orange text",
        SnapshotColor::Rgb(255, 165, 0),
        false,
    );

    // Row 5: color cube strip (indices 16–51).
    if rows > 5 {
        for col in 0..cols.min(36) {
            let cell = &mut cells[5 * cols + col];
            cell.c = '#';
            cell.fg = SnapshotColor::Indexed(16 + col as u8);
        }
    }

    RenderSnapshot {
        cells,
        cols,
        rows,
        cursor_row: 0,
        cursor_col: 0,
        cursor_visible: true,
        cursor_shape: alacritty_terminal::vte::ansi::CursorShape::Block,
    }
}

/// Write a string into a row of the snapshot cell buffer.
fn write_row(
    cells: &mut [SnapshotCell],
    row: usize,
    cols: usize,
    text: &str,
    fg: SnapshotColor,
    bold: bool,
) {
    for (col, ch) in text.chars().enumerate() {
        if col >= cols {
            break;
        }
        let cell = &mut cells[row * cols + col];
        cell.c = ch;
        cell.fg = fg;
        cell.bold = bold;
    }
}
