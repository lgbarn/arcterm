//! Window example: renders a test grid using arcterm-render.
//!
//! Run with: cargo run --package arcterm-render --example window

use std::sync::Arc;

use arcterm_core::{Cell, CellAttrs, Color, Grid, GridSize};
use arcterm_render::Renderer;
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
    grid: Grid,
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

        let renderer = Renderer::new(window.clone());

        // Build a test grid with "Hello, Arcterm!" and colored rows.
        let size = renderer.grid_size_for_window(
            window.inner_size().width,
            window.inner_size().height,
            window.scale_factor(),
        );
        let grid = build_test_grid(size);

        self.state = Some(AppState {
            window,
            renderer,
            grid,
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
                let new_size = state.renderer.grid_size_for_window(
                    size.width,
                    size.height,
                    state.window.scale_factor(),
                );
                state.grid = build_test_grid(new_size);
                state.window.request_redraw();
            }

            WindowEvent::RedrawRequested => {
                state
                    .renderer
                    .render_frame(&state.grid, state.window.scale_factor());
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
// Test grid construction
// ---------------------------------------------------------------------------

/// Build a grid filled with demo content.
fn build_test_grid(size: GridSize) -> Grid {
    let mut grid = Grid::new(size);
    let cols = size.cols;

    // Row 0: "Hello, Arcterm!" in default colors.
    write_row(&mut grid, 0, "Hello, Arcterm!", &CellAttrs::default(), cols);

    // Row 1: bright red label.
    let red_attrs = CellAttrs {
        fg: Color::Indexed(9), // bright red
        ..Default::default()
    };
    write_row(&mut grid, 1, "  Red text row", &red_attrs, cols);

    // Row 2: bright green label.
    let green_attrs = CellAttrs {
        fg: Color::Indexed(10), // bright green
        ..Default::default()
    };
    write_row(&mut grid, 2, "  Green text row", &green_attrs, cols);

    // Row 3: bright cyan label.
    let cyan_attrs = CellAttrs {
        fg: Color::Indexed(14), // bright cyan
        ..Default::default()
    };
    write_row(&mut grid, 3, "  Cyan text row", &cyan_attrs, cols);

    // Row 4: true-color RGB example.
    let rgb_attrs = CellAttrs {
        fg: Color::Rgb(255, 165, 0), // orange
        ..Default::default()
    };
    write_row(&mut grid, 4, "  RGB orange text", &rgb_attrs, cols);

    // Row 5: color cube strip (indices 16–51).
    if size.rows > 5 {
        for col in 0..cols.min(36) {
            let cell = grid.cell_mut(5, col);
            cell.c = '#';
            cell.attrs = CellAttrs {
                fg: Color::Indexed(16 + col as u8),
                ..Default::default()
            };
        }
    }

    grid
}

/// Write a string into a grid row, padding with spaces.
fn write_row(grid: &mut Grid, row: usize, text: &str, attrs: &CellAttrs, cols: usize) {
    if row >= grid.size.rows {
        return;
    }
    for (col, ch) in text.chars().enumerate() {
        if col >= cols {
            break;
        }
        let cell = grid.cell_mut(row, col);
        *cell = Cell {
            c: ch,
            attrs: *attrs,
            dirty: true,
        };
    }
}
