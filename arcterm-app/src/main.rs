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

mod colors;
mod config;
mod detect;
mod input;
mod keymap;
mod layout;
mod neovim;
mod palette;
mod selection;
mod tab;
mod terminal;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::event_loop::ControlFlow;

#[cfg(feature = "latency-trace")]
use std::time::Instant as TraceInstant;

use arcterm_core::{Cell, CellAttrs, Color, CursorPos, GridSize};
use arcterm_render::{OverlayQuad, PaneRenderInfo, RenderPalette, Renderer};
use keymap::{KeyAction, KeymapHandler};
use palette::PaletteState;
use layout::{Axis, Direction, PaneId, PaneNode, PixelRect};
use selection::{generate_selection_quads, pixel_to_cell, Clipboard, Selection, SelectionMode, SelectionQuad};
use tab::TabManager;
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

/// Width of pane borders in physical pixels (logical border_width * scale).
const BORDER_PX: f32 = 2.0;

/// Border color for unfocused pane borders (dark grey, RGBA).
const BORDER_COLOR_NORMAL: [u8; 4] = [60, 60, 60, 255];

/// Border color for the split containing the focused pane (accent, RGBA).
const BORDER_COLOR_FOCUS: [u8; 4] = [130, 100, 200, 255];

/// Amount to adjust the split ratio per resize keypress.
const RESIZE_DELTA: f32 = 0.05;

/// Border drag threshold in physical pixels — within this distance of a border
/// edge the cursor is treated as "on the border" for drag-to-resize.
const BORDER_DRAG_THRESHOLD: f32 = 4.0;

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

    // ---- multiplexer core ----
    /// All open terminal instances, keyed by pane ID.
    panes: HashMap<PaneId, Terminal>,
    /// PTY byte channels for every open pane.
    pty_channels: HashMap<PaneId, mpsc::Receiver<Vec<u8>>>,
    /// Tab manager: tab labels, per-tab focused pane, per-tab zoom state.
    tab_manager: TabManager,
    /// Layout trees for each tab, index-aligned with `tab_manager.tabs`.
    tab_layouts: Vec<PaneNode>,
    /// Leader-key state machine.
    keymap: KeymapHandler,

    /// Set to `true` once ALL PTY channels close (all panes have exited).
    shell_exited: bool,

    // ---- configuration ----
    config: config::ArctermConfig,
    config_rx: Option<std::sync::mpsc::Receiver<config::ArctermConfig>>,

    // ---- selection & clipboard ----
    selection: Selection,
    clipboard: Option<Clipboard>,
    /// Last known physical cursor position (pixels).
    last_cursor_position: (f64, f64),
    /// Timestamp of the last left-button press (for multi-click detection).
    last_click_time: Option<Instant>,
    /// Consecutive click count: 1 = single, 2 = double, 3 = triple.
    click_count: u32,

    // ---- border drag state ----
    /// If Some, we are dragging the border that contains this pane to resize.
    drag_pane: Option<PaneId>,

    // ---- selection rendering ----
    selection_quads: Vec<SelectionQuad>,

    // ---- performance / control flow ----
    idle_cycles: u32,
    fps_last_log: Instant,
    fps_frame_count: u32,

    /// Command palette state; `None` when the palette is closed.
    palette_mode: Option<PaletteState>,

    // ---- Neovim integration ----
    /// Cached Neovim detection state for each pane.  Updated lazily on
    /// `NavigatePane` events with a 2-second TTL to avoid syscall spam.
    nvim_states: HashMap<PaneId, neovim::NeovimState>,
}

impl AppState {
    // -----------------------------------------------------------------------
    // Active-tab helpers
    // -----------------------------------------------------------------------

    /// The `PaneId` of the focused pane in the active tab.
    fn focused_pane(&self) -> PaneId {
        self.tab_manager.active_tab().focus
    }

    /// Set the focused pane on the active tab.
    fn set_focused_pane(&mut self, id: PaneId) {
        self.tab_manager.active_tab_mut().focus = id;
    }

    /// The layout tree for the active tab.
    fn active_layout(&self) -> &PaneNode {
        &self.tab_layouts[self.tab_manager.active]
    }

    /// A mutable reference to the layout tree for the active tab.
    #[allow(dead_code)] // Available for future use
    fn active_layout_mut(&mut self) -> &mut PaneNode {
        let active = self.tab_manager.active;
        &mut self.tab_layouts[active]
    }

    // -----------------------------------------------------------------------
    // Geometry helpers
    // -----------------------------------------------------------------------

    /// Compute the pixel rect available for pane content (below tab bar when shown).
    fn pane_area(&self) -> PixelRect {
        let w = self.renderer.gpu.surface_config.width as f32;
        let h = self.renderer.gpu.surface_config.height as f32;
        let sf = self.window.scale_factor() as f32;

        let tab_h = if self.config.multiplexer.show_tab_bar && self.tab_manager.tab_count() > 1 {
            arcterm_render::tab_bar_height(&self.renderer.text.cell_size, sf)
        } else {
            0.0
        };

        PixelRect { x: 0.0, y: tab_h, width: w, height: h - tab_h }
    }

    /// Compute pixel rects for all panes in the active tab.
    fn compute_pane_rects(&self) -> HashMap<PaneId, PixelRect> {
        let available = self.pane_area();
        let tab = self.tab_manager.active_tab();

        if let Some(zoomed_id) = tab.zoomed {
            self.active_layout().compute_zoomed_rect(zoomed_id, available)
        } else {
            self.active_layout().compute_rects(available, BORDER_PX)
        }
    }

    /// Given a rect, compute the grid size (rows × cols) for a pane.
    fn grid_size_for_rect(&self, rect: PixelRect) -> GridSize {
        let sf = self.window.scale_factor() as f32;
        let cell_w = self.renderer.text.cell_size.width * sf;
        let cell_h = self.renderer.text.cell_size.height * sf;
        if cell_w <= 0.0 || cell_h <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return GridSize::new(1, 1);
        }
        let cols = (rect.width / cell_w).floor() as usize;
        let rows = (rect.height / cell_h).floor() as usize;
        GridSize::new(rows.max(1), cols.max(1))
    }

    // -----------------------------------------------------------------------
    // Spawn a new pane
    // -----------------------------------------------------------------------

    /// Spawn a new Terminal + PTY, insert into the pane and channel maps, and
    /// return its `PaneId`.  The grid is sized to `size`.
    fn spawn_pane(&mut self, size: GridSize) -> PaneId {
        let id = PaneId::next();
        match Terminal::new(size, self.config.shell.clone()) {
            Ok((mut terminal, rx)) => {
                terminal.grid_mut().max_scrollback = self.config.scrollback_lines;
                self.panes.insert(id, terminal);
                self.pty_channels.insert(id, rx);
            }
            Err(e) => {
                log::error!("Failed to create PTY for pane {:?}: {e}", id);
            }
        }
        id
    }
}

struct App {
    state: Option<AppState>,
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

        let cfg = config::ArctermConfig::load();
        let config_rx = config::watch_config();
        log::info!(
            "config: font_size={}, scrollback_lines={}, color_scheme={}",
            cfg.font_size,
            cfg.scrollback_lines,
            cfg.color_scheme,
        );

        let window = Arc::new(
            event_loop
                .create_window(window_attrs)
                .expect("failed to create window"),
        );

        let mut renderer = Renderer::new(window.clone(), cfg.font_size);

        let palette = palette_from_config(&cfg);
        log::info!("config: color_scheme={:?}", cfg.color_scheme);
        renderer.set_palette(palette);

        // Compute initial grid size for a full-window single pane.
        let win_size = window.inner_size();
        let initial_size = renderer.grid_size_for_window(
            win_size.width,
            win_size.height,
            window.scale_factor(),
        );

        // Spawn the first pane.
        let first_id = PaneId::next();
        let (mut terminal, pty_rx) =
            Terminal::new(initial_size, cfg.shell.clone()).unwrap_or_else(|e| {
                log::error!("Failed to create PTY session: {e}");
                std::process::exit(1);
            });
        terminal.grid_mut().max_scrollback = cfg.scrollback_lines;

        let mut panes = HashMap::new();
        panes.insert(first_id, terminal);

        let mut pty_channels = HashMap::new();
        pty_channels.insert(first_id, pty_rx);

        let tab_manager = TabManager::new(first_id);
        // Matching layout tree for the first (only) tab.
        let tab_layouts = vec![PaneNode::Leaf { pane_id: first_id }];

        let keymap = KeymapHandler::new(cfg.multiplexer.leader_timeout_ms);

        let clipboard = Clipboard::new()
            .map_err(|e| log::warn!("Clipboard unavailable: {e}"))
            .ok();

        self.state = Some(AppState {
            window,
            renderer,
            panes,
            pty_channels,
            tab_manager,
            tab_layouts,
            keymap,
            shell_exited: false,
            config: cfg,
            config_rx,
            selection: Selection::default(),
            clipboard,
            last_cursor_position: (0.0, 0.0),
            last_click_time: None,
            click_count: 0,
            drag_pane: None,
            selection_quads: Vec::new(),
            idle_cycles: 0,
            fps_last_log: Instant::now(),
            fps_frame_count: 0,
            palette_mode: None,
            nvim_states: HashMap::new(),
        });
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(state) = &mut self.state else {
            return;
        };

        if state.shell_exited {
            return;
        }

        // ------------------------------------------------------------------
        // Config hot-reload
        // ------------------------------------------------------------------
        if let Some(rx) = &state.config_rx {
            loop {
                match rx.try_recv() {
                    Ok(new_cfg) => {
                        if (new_cfg.font_size - state.config.font_size).abs() > f32::EPSILON {
                            log::info!(
                                "config: font_size changed ({} → {}): restart required",
                                state.config.font_size,
                                new_cfg.font_size,
                            );
                        }

                        if new_cfg.scrollback_lines != state.config.scrollback_lines {
                            log::info!(
                                "config: scrollback_lines changed ({} → {})",
                                state.config.scrollback_lines,
                                new_cfg.scrollback_lines,
                            );
                            // Update all live panes.
                            for terminal in state.panes.values_mut() {
                                terminal.grid_mut().max_scrollback = new_cfg.scrollback_lines;
                            }
                        }

                        if new_cfg.color_scheme != state.config.color_scheme
                            || new_cfg.colors.foreground != state.config.colors.foreground
                            || new_cfg.colors.background != state.config.colors.background
                            || new_cfg.colors.cursor != state.config.colors.cursor
                            || new_cfg.colors.red != state.config.colors.red
                            || new_cfg.colors.green != state.config.colors.green
                            || new_cfg.colors.blue != state.config.colors.blue
                            || new_cfg.colors.yellow != state.config.colors.yellow
                            || new_cfg.colors.magenta != state.config.colors.magenta
                            || new_cfg.colors.cyan != state.config.colors.cyan
                            || new_cfg.colors.white != state.config.colors.white
                            || new_cfg.colors.black != state.config.colors.black
                            || new_cfg.colors.bright_red != state.config.colors.bright_red
                            || new_cfg.colors.bright_green != state.config.colors.bright_green
                            || new_cfg.colors.bright_blue != state.config.colors.bright_blue
                            || new_cfg.colors.bright_yellow != state.config.colors.bright_yellow
                            || new_cfg.colors.bright_magenta != state.config.colors.bright_magenta
                            || new_cfg.colors.bright_cyan != state.config.colors.bright_cyan
                            || new_cfg.colors.bright_white != state.config.colors.bright_white
                            || new_cfg.colors.bright_black != state.config.colors.bright_black
                        {
                            log::info!(
                                "config: color_scheme changed ({:?} → {:?}), reloading palette",
                                state.config.color_scheme,
                                new_cfg.color_scheme,
                            );
                            let new_palette = palette_from_config(&new_cfg);
                            state.renderer.set_palette(new_palette);
                            state.window.request_redraw();
                        }

                        state.config = new_cfg;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        log::warn!("config: watcher channel disconnected");
                        break;
                    }
                }
            }
        }

        // ------------------------------------------------------------------
        // Poll ALL PTY channels (all tabs, all panes).
        // ------------------------------------------------------------------
        let mut got_data = false;
        let mut closed_panes: Vec<PaneId> = Vec::new();

        // Collect pane IDs to avoid borrow checker conflicts.
        let pane_ids: Vec<PaneId> = state.pty_channels.keys().copied().collect();

        for id in pane_ids {
            'drain: loop {
                let Some(rx) = state.pty_channels.get_mut(&id) else {
                    break 'drain;
                };
                match rx.try_recv() {
                    Ok(bytes) => {
                        #[cfg(feature = "latency-trace")]
                        let t0 = TraceInstant::now();

                        if let Some(terminal) = state.panes.get_mut(&id) {
                            terminal.process_pty_output(&bytes);
                        }
                        got_data = true;

                        #[cfg(feature = "latency-trace")]
                        log::debug!(
                            "[latency] PTY output processed: {} bytes in {:?}",
                            bytes.len(),
                            t0.elapsed()
                        );
                    }
                    Err(mpsc::error::TryRecvError::Empty) => break 'drain,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        log::info!("PTY channel closed for pane {:?}", id);
                        closed_panes.push(id);
                        break 'drain;
                    }
                }
            }
        }

        // Remove closed pane channels.
        for id in closed_panes {
            state.pty_channels.remove(&id);
        }

        // When ALL channels are gone, the session has ended.
        if state.pty_channels.is_empty() {
            log::info!("All PTY channels closed — shell has exited");
            state.shell_exited = true;
            state.window.request_redraw();
        }

        if got_data {
            // Clear selection and scroll-to-live on the focused pane only.
            let focused = state.focused_pane();
            if let Some(terminal) = state.panes.get_mut(&focused) {
                let grid = terminal.grid_mut();
                if grid.scroll_offset > 0 {
                    state.selection.clear();
                    grid.scroll_offset = 0;
                }
            }
            state.window.request_redraw();
            state.idle_cycles = 0;
            event_loop.set_control_flow(ControlFlow::Poll);
        } else {
            state.idle_cycles = state.idle_cycles.saturating_add(1);
            if state.idle_cycles >= 3 {
                event_loop.set_control_flow(ControlFlow::Wait);
            } else {
                event_loop.set_control_flow(ControlFlow::Poll);
            }
        }

        // Drain pending DSR/DA replies for ALL panes and write them back.
        let pane_ids: Vec<PaneId> = state.panes.keys().copied().collect();
        for id in pane_ids {
            if let Some(terminal) = state.panes.get_mut(&id) {
                let replies = terminal.take_pending_replies();
                for reply in replies {
                    terminal.write_input(&reply);
                }
            }
        }

        // Wire window title from focused pane.
        {
            let focused = state.focused_pane();
            if let Some(terminal) = state.panes.get(&focused) {
                let title = terminal.grid().title().map(|t| t.to_string());
                if let Some(t) = title
                    && !t.is_empty()
                {
                    state.window.set_title(&t);
                }
            }
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

            // -----------------------------------------------------------------
            // Window resize — recompute rects and resize all panes.
            // -----------------------------------------------------------------
            WindowEvent::Resized(size) => {
                if size.width == 0 || size.height == 0 {
                    return;
                }
                state.renderer.resize(size.width, size.height);

                // Resize every pane to its new rect.
                let rects = state.compute_pane_rects();
                for (id, rect) in &rects {
                    let new_size = state.grid_size_for_rect(*rect);
                    if let Some(terminal) = state.panes.get_mut(id) {
                        terminal.resize(new_size);
                    }
                }
                state.window.request_redraw();
            }

            // -----------------------------------------------------------------
            // Mouse cursor movement — extend drag selection OR border drag.
            // -----------------------------------------------------------------
            WindowEvent::CursorMoved { position, .. } => {
                state.last_cursor_position = (position.x, position.y);

                // Border drag resize.
                if let Some(drag_id) = state.drag_pane {
                    let rects = state.compute_pane_rects();
                    if let Some(drag_rect) = rects.get(&drag_id) {
                        let px = position.x as f32;
                        // Determine delta based on whether it's closer to left/right or top/bottom edge.
                        let left_dist = (px - drag_rect.x).abs();
                        let right_dist = (px - (drag_rect.x + drag_rect.width)).abs();
                        let is_right_edge = right_dist < left_dist;

                        let delta = if is_right_edge {
                            // Dragging the right edge: moving right increases ratio.
                            let available_w = rects.values().map(|r| r.width).sum::<f32>() + rects.len() as f32 * BORDER_PX;
                            if available_w > 0.0 {
                                // Compute fraction of new position vs total width.
                                let new_ratio = (px - drag_rect.x) / (available_w - BORDER_PX);
                                let old_ratio = drag_rect.width / (available_w - BORDER_PX);
                                new_ratio - old_ratio
                            } else {
                                0.0
                            }
                        } else {
                            0.0
                        };

                        if delta.abs() > 0.001 {
                            let active = state.tab_manager.active;
                            state.tab_layouts[active].resize_split(drag_id, delta);
                            state.window.request_redraw();
                        }
                    }
                    return;
                }

                // Extend drag selection.
                if state.selection.mode != SelectionMode::None {
                    let focused = state.focused_pane();
                    let rects = state.compute_pane_rects();
                    if let Some(rect) = rects.get(&focused) {
                        let cell = cursor_to_cell_in_rect(state, position.x, position.y, *rect);
                        state.selection.update(cell);
                        state.window.request_redraw();
                    }
                }
            }

            // -----------------------------------------------------------------
            // Mouse button press/release — click-to-focus, selection, border drag.
            // -----------------------------------------------------------------
            WindowEvent::MouseInput { state: btn_state, button, .. } => {
                if button == MouseButton::Left {
                    match btn_state {
                        ElementState::Pressed => {
                            let (px, py) = state.last_cursor_position;
                            let pxf = px as f32;
                            let pyf = py as f32;

                            let rects = state.compute_pane_rects();

                            // --- Tab bar click ---
                            let sf = state.window.scale_factor() as f32;
                            let tab_h = if state.config.multiplexer.show_tab_bar && state.tab_manager.tab_count() > 1 {
                                arcterm_render::tab_bar_height(&state.renderer.text.cell_size, sf)
                            } else {
                                0.0
                            };

                            if tab_h > 0.0 && pyf < tab_h {
                                let win_w = state.renderer.gpu.surface_config.width as f32;
                                let tab_count = state.tab_manager.tab_count();
                                let tab_w = win_w / tab_count as f32;
                                let clicked_tab = (pxf / tab_w).floor() as usize;
                                let clicked_tab = clicked_tab.min(tab_count - 1);
                                state.tab_manager.switch_to(clicked_tab);
                                state.window.request_redraw();
                                return;
                            }

                            // --- Border drag detection ---
                            let mut on_border = false;
                            for (&id, rect) in &rects {
                                // Check if we're near the right edge of this pane (which is a border).
                                let right_edge = rect.x + rect.width;
                                if (pxf - right_edge).abs() < BORDER_DRAG_THRESHOLD
                                    && pyf >= rect.y
                                    && pyf < rect.y + rect.height
                                {
                                    state.drag_pane = Some(id);
                                    on_border = true;
                                    break;
                                }
                                // Check bottom edge.
                                let bottom_edge = rect.y + rect.height;
                                if (pyf - bottom_edge).abs() < BORDER_DRAG_THRESHOLD
                                    && pxf >= rect.x
                                    && pxf < rect.x + rect.width
                                {
                                    state.drag_pane = Some(id);
                                    on_border = true;
                                    break;
                                }
                            }

                            if on_border {
                                return;
                            }

                            // --- Click-to-focus ---
                            let clicked_pane = rects.iter().find_map(|(&id, rect)| {
                                if rect.contains(pxf, pyf) { Some(id) } else { None }
                            });

                            if let Some(new_focus) = clicked_pane {
                                let current_focus = state.focused_pane();
                                if new_focus != current_focus {
                                    state.set_focused_pane(new_focus);
                                    state.selection.clear();
                                    state.window.request_redraw();
                                }
                            }

                            // --- Selection ---
                            let focused = state.focused_pane();
                            if let Some(&focus_rect) = rects.get(&focused)
                                && focus_rect.contains(pxf, pyf)
                            {
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

                                let cell = cursor_to_cell_in_rect(state, px, py, focus_rect);
                                let mode = match state.click_count {
                                    1 => SelectionMode::Character,
                                    2 => SelectionMode::Word,
                                    _ => SelectionMode::Line,
                                };
                                state.selection.start(cell, mode);
                                state.window.request_redraw();
                            }
                        }
                        ElementState::Released => {
                            // Stop border drag.
                            state.drag_pane = None;
                            // Keep the selection visible but stop extending it.
                        }
                    }
                }
            }

            // -----------------------------------------------------------------
            // Mouse wheel — scroll the focused pane's viewport.
            // -----------------------------------------------------------------
            WindowEvent::MouseWheel { delta, .. } => {
                let lines: i32 = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as i32,
                    MouseScrollDelta::PixelDelta(pos) => {
                        let cell_h = state.renderer.text.cell_size.height as f64;
                        (pos.y / cell_h).round() as i32
                    }
                };

                if lines != 0 {
                    let focused = state.focused_pane();
                    if let Some(terminal) = state.panes.get_mut(&focused) {
                        let grid = terminal.grid_mut();
                        let max_offset = grid.scrollback.len();
                        let current = grid.scroll_offset as i32;
                        let new_offset = (current - lines * SCROLL_LINES_PER_TICK as i32)
                            .clamp(0, max_offset as i32) as usize;
                        grid.scroll_offset = new_offset;
                        state.window.request_redraw();
                    }
                }
            }

            // -----------------------------------------------------------------
            // Redraw — render all panes, borders, and tab bar.
            // -----------------------------------------------------------------
            WindowEvent::RedrawRequested => {
                // FPS counter.
                state.fps_frame_count += 1;
                let fps_elapsed = state.fps_last_log.elapsed();
                if fps_elapsed >= Duration::from_secs(5) {
                    let fps = state.fps_frame_count as f64 / fps_elapsed.as_secs_f64();
                    log::debug!("fps: {:.1} ({} frames in {:.1}s)", fps, state.fps_frame_count, fps_elapsed.as_secs_f64());
                    state.fps_frame_count = 0;
                    state.fps_last_log = Instant::now();
                }

                #[cfg(feature = "latency-trace")]
                let t0 = TraceInstant::now();

                let scale = state.window.scale_factor();
                let sf = scale as f32;
                let rects = state.compute_pane_rects();
                let focused = state.focused_pane();

                // Recompute selection quads for the focused pane.
                {
                    if let (Some(&focus_rect), Some(terminal)) =
                        (rects.get(&focused), state.panes.get(&focused))
                    {
                        let grid = terminal.grid();
                        let cell_w = state.renderer.text.cell_size.width;
                        let cell_h = state.renderer.text.cell_size.height;
                        state.selection_quads = generate_selection_quads(
                            &state.selection,
                            grid.size.rows,
                            grid.size.cols,
                            cell_w,
                            cell_h,
                            sf,
                        );
                        let _ = focus_rect; // rect is used contextually for offset in future
                        log::trace!(
                            "selection quads: {} rect(s) for mode {:?}",
                            state.selection_quads.len(),
                            state.selection.mode
                        );
                    }
                }

                // Collect overlay quads: borders + tab bar.
                let mut overlay_quads: Vec<OverlayQuad> = Vec::new();

                // Tab bar (only when > 1 tab and show_tab_bar is true).
                if state.config.multiplexer.show_tab_bar && state.tab_manager.tab_count() > 1 {
                    let win_w = state.renderer.gpu.surface_config.width as f32;
                    let tab_quads = arcterm_render::render_tab_bar_quads(
                        state.tab_manager.tab_count(),
                        state.tab_manager.active,
                        &state.renderer.text.cell_size,
                        sf,
                        win_w,
                        &state.renderer.palette,
                    );
                    for q in tab_quads {
                        overlay_quads.push(OverlayQuad {
                            rect: q.rect,
                            color: q.color,
                        });
                    }
                }

                // Pane borders from the active layout tree.
                {
                    let available = state.pane_area();
                    let tab = state.tab_manager.active_tab();
                    // Only draw borders when NOT zoomed (zoomed = single pane fills area).
                    if tab.zoomed.is_none() {
                        let border_quads = state.active_layout().compute_border_quads(
                            available,
                            BORDER_PX,
                            focused,
                            BORDER_COLOR_NORMAL,
                            BORDER_COLOR_FOCUS,
                        );
                        for bq in border_quads {
                            overlay_quads.push(OverlayQuad {
                                rect: [bq.rect.x, bq.rect.y, bq.rect.width, bq.rect.height],
                                color: [
                                    bq.color[0] as f32 / 255.0,
                                    bq.color[1] as f32 / 255.0,
                                    bq.color[2] as f32 / 255.0,
                                    bq.color[3] as f32 / 255.0,
                                ],
                            });
                        }
                    }
                }

                // Build pane render infos.
                let mut pane_infos: Vec<PaneRenderInfo<'_>> = Vec::new();

                if state.shell_exited {
                    // Show exit banner on the focused pane (or first available).
                    let target_id = focused;
                    if let Some(terminal) = state.panes.get(&target_id) {
                        let mut display = terminal.grid().clone();
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
                        // Render immediately as a single-pane frame.
                        state.renderer.render_frame(&display, scale);
                        return;
                    }
                }

                // Normal multi-pane render.
                // We need to collect grid refs while panes is borrowed.
                // Use a Vec of (rect, grid_clone) to avoid lifetime issues.
                let pane_frames: Vec<(PixelRect, arcterm_core::Grid)> = rects
                    .iter()
                    .filter_map(|(id, rect)| {
                        if rect.width <= 0.0 || rect.height <= 0.0 {
                            return None;
                        }
                        state.panes.get(id).map(|t| (*rect, t.grid().clone()))
                    })
                    .collect();

                for (rect, grid) in &pane_frames {
                    pane_infos.push(PaneRenderInfo {
                        grid,
                        rect: [rect.x, rect.y, rect.width, rect.height],
                        structured_blocks: &[],
                    });
                }

                // Build palette overlay quads and text when the palette is open.
                let mut palette_text: Vec<(String, f32, f32)> = Vec::new();
                if let Some(pal) = &state.palette_mode {
                    let win_w = state.renderer.gpu.surface_config.width as f32;
                    let win_h = state.renderer.gpu.surface_config.height as f32;
                    let cell_w = state.renderer.text.cell_size.width;
                    let cell_h = state.renderer.text.cell_size.height;
                    let pal_sf = sf;

                    // Quads.
                    let pal_quads = pal.render_quads(win_w, win_h, cell_w, cell_h, pal_sf);
                    for pq in pal_quads {
                        overlay_quads.push(OverlayQuad {
                            rect: pq.rect,
                            color: pq.color,
                        });
                    }

                    // Text.
                    let pal_texts = pal.render_text_content(win_w, win_h, cell_w, cell_h, pal_sf);
                    for pt in pal_texts {
                        palette_text.push((pt.text, pt.x, pt.y));
                    }
                }

                state.renderer.render_multipane(&pane_infos, &overlay_quads, &palette_text, scale);

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

            // -----------------------------------------------------------------
            // Keyboard input — routed through KeymapHandler.
            // -----------------------------------------------------------------
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    #[cfg(feature = "latency-trace")]
                    let t0 = TraceInstant::now();

                    let super_key = self.modifiers.super_key();

                    // Cmd+C — copy selection to clipboard (before keymap).
                    if super_key {
                        use winit::keyboard::Key;
                        if let Key::Character(ref s) = event.logical_key {
                            match s.as_str() {
                                "c" | "C" => {
                                    let focused = state.focused_pane();
                                    if let Some(terminal) = state.panes.get(&focused) {
                                        let text = state.selection.extract_text(terminal.grid());
                                        if !text.is_empty()
                                            && let Some(cb) = &mut state.clipboard
                                            && let Err(e) = cb.copy(&text)
                                        {
                                            log::warn!("Clipboard copy failed: {e}");
                                        }
                                    }
                                    return;
                                }
                                "v" | "V" => {
                                    if let Some(cb) = &mut state.clipboard {
                                        match cb.paste() {
                                            Ok(text) => {
                                                let focused = state.focused_pane();
                                                if let Some(terminal) = state.panes.get_mut(&focused) {
                                                    let bracketed = terminal.grid_state().modes.bracketed_paste;
                                                    if bracketed {
                                                        let mut payload = b"\x1b[200~".to_vec();
                                                        payload.extend_from_slice(text.as_bytes());
                                                        payload.extend_from_slice(b"\x1b[201~");
                                                        terminal.write_input(&payload);
                                                    } else {
                                                        terminal.write_input(text.as_bytes());
                                                    }
                                                    state.window.request_redraw();
                                                }
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

                    // Palette is modal — route ALL key input through it when open.
                    if let Some(palette) = &mut state.palette_mode {
                        use palette::PaletteEvent;
                        let palette_event = palette.handle_input(&event, self.modifiers);
                        match palette_event {
                            PaletteEvent::Close => {
                                state.palette_mode = None;
                                state.window.request_redraw();
                            }
                            PaletteEvent::Execute(palette_action) => {
                                let key_action = palette_action.to_key_action();
                                state.palette_mode = None;
                                // Dispatch the converted KeyAction through the same
                                // path used by regular keymap bindings.
                                execute_key_action(state, event_loop, key_action);
                                state.window.request_redraw();
                            }
                            PaletteEvent::Consumed => {
                                state.window.request_redraw();
                            }
                        }
                        return;
                    }

                    // Route through the keymap handler.
                    let focused_id = state.focused_pane();
                    let app_cursor = state
                        .panes
                        .get(&focused_id)
                        .map(|t| t.grid_state().modes.app_cursor_keys)
                        .unwrap_or(false);

                    let action = state.keymap.handle_key(&event, self.modifiers, app_cursor);

                    match action {
                        KeyAction::Forward(bytes) => {
                            #[cfg(feature = "latency-trace")]
                            log::debug!(
                                "[latency] key → PTY write ({} bytes) after {:?}",
                                bytes.len(),
                                t0.elapsed()
                            );

                            if let Some(terminal) = state.panes.get_mut(&focused_id) {
                                terminal.write_input(&bytes);
                            }
                            state.window.request_redraw();
                        }

                        KeyAction::NavigatePane(dir) => {
                            // ── Neovim-aware pane crossing ──────────────────
                            // 1. Retrieve (or refresh) the cached Neovim state
                            //    for the focused pane.
                            let child_pid = state
                                .panes
                                .get(&focused_id)
                                .and_then(|t| t.child_pid());

                            let nvim_state = {
                                let needs_refresh = state
                                    .nvim_states
                                    .get(&focused_id)
                                    .map(|s| !s.is_fresh())
                                    .unwrap_or(true);

                                if needs_refresh {
                                    let fresh = neovim::NeovimState::check(child_pid);
                                    state.nvim_states.insert(focused_id, fresh);
                                }

                                // Borrow the state immutably for the check below.
                                let s = state.nvim_states.get(&focused_id).unwrap();
                                (s.is_nvim, s.socket_path.clone())
                            };

                            // 2. If the pane is running Neovim and we have a
                            //    socket path, query whether Neovim has a split
                            //    in the requested direction.
                            let nvim_consumed = if nvim_state.0 {
                                if let Some(ref socket_path) = nvim_state.1 {
                                    // Use block_in_place — we are already inside
                                    // the Tokio runtime context established in
                                    // main().  This runs the synchronous socket
                                    // I/O on the current thread without blocking
                                    // the async executor.
                                    tokio::task::block_in_place(|| {
                                        match neovim::NvimRpcClient::connect(socket_path) {
                                            Ok(mut client) => {
                                                match neovim::has_nvim_neighbor(&mut client, dir) {
                                                    Ok(true) => {
                                                        // Neovim has a split in this
                                                        // direction — forward the key.
                                                        let ctrl_byte: &[u8] = match dir {
                                                            Direction::Left  => &[0x08], // Ctrl+h
                                                            Direction::Down  => &[0x0A], // Ctrl+j
                                                            Direction::Up    => &[0x0B], // Ctrl+k
                                                            Direction::Right => &[0x0C], // Ctrl+l
                                                        };
                                                        if let Some(terminal) =
                                                            state.panes.get_mut(&focused_id)
                                                        {
                                                            terminal.write_input(ctrl_byte);
                                                        }
                                                        true // consumed by Neovim
                                                    }
                                                    Ok(false) => false, // fall through
                                                    Err(e) => {
                                                        log::debug!(
                                                            "nvim RPC query failed for {:?}: {e}",
                                                            focused_id
                                                        );
                                                        false // fall through
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                log::debug!(
                                                    "nvim socket connect failed for {:?}: {e}",
                                                    focused_id
                                                );
                                                false // fall through
                                            }
                                        }
                                    })
                                } else {
                                    false // no socket → fall through
                                }
                            } else {
                                false // not nvim → fall through
                            };

                            // 3. If Neovim did not consume the key, use
                            //    arcterm's layout-based navigation.
                            if !nvim_consumed {
                                let rects = state.compute_pane_rects();
                                if let Some(new_focus) = state
                                    .active_layout()
                                    .focus_in_direction(focused_id, dir, &rects)
                                {
                                    state.set_focused_pane(new_focus);
                                    state.selection.clear();
                                    state.window.request_redraw();
                                }
                            }
                        }

                        KeyAction::Split(axis) => {
                            let rects = state.compute_pane_rects();
                            let focused = focused_id;
                            let focused_rect = rects.get(&focused).copied().unwrap_or(PixelRect {
                                x: 0.0,
                                y: 0.0,
                                width: 800.0,
                                height: 600.0,
                            });

                            // Compute size for the new pane (half of focused pane's rect).
                            let new_rect = match axis {
                                Axis::Horizontal => PixelRect {
                                    width: focused_rect.width / 2.0,
                                    ..focused_rect
                                },
                                Axis::Vertical => PixelRect {
                                    height: focused_rect.height / 2.0,
                                    ..focused_rect
                                },
                            };
                            let new_size = state.grid_size_for_rect(new_rect);
                            let new_id = state.spawn_pane(new_size);

                            // Also resize the original pane to its new half.
                            let orig_size = state.grid_size_for_rect(new_rect);
                            if let Some(terminal) = state.panes.get_mut(&focused) {
                                terminal.resize(orig_size);
                            }

                            // Update the layout tree.
                            let active = state.tab_manager.active;
                            state.tab_layouts[active].split(focused, axis, new_id);

                            // Focus the new pane.
                            state.set_focused_pane(new_id);
                            state.window.request_redraw();
                        }

                        KeyAction::ClosePane => {
                            let focused = focused_id;
                            let active = state.tab_manager.active;
                            let pane_count = state.tab_layouts[active].all_pane_ids().len();

                            if pane_count <= 1 {
                                // Last pane in the tab — close the tab if possible.
                                let removed_ids = state.tab_manager.close_tab(active);
                                // Remove layout for this tab.
                                if active < state.tab_layouts.len() {
                                    state.tab_layouts.remove(active);
                                }
                                for id in removed_ids {
                                    let lid = id;
                                    state.panes.remove(&lid);
                                    state.pty_channels.remove(&lid);
                                    state.nvim_states.remove(&lid);
                                }
                                // If no tabs left, exit.
                                if state.tab_manager.tab_count() == 0 {
                                    event_loop.exit();
                                    return;
                                }
                                // Focus the first pane of the now-active tab.
                                let new_focus = state.tab_manager.active_tab().focus;
                                state.set_focused_pane(new_focus);
                            } else {
                                // Multiple panes: promote sibling.
                                let replacement = state.tab_layouts[active].close(focused);
                                if let Some(new_root) = replacement {
                                    state.tab_layouts[active] = new_root;
                                }

                                // Remove the terminal and channel.
                                state.panes.remove(&focused);
                                state.pty_channels.remove(&focused);
                                state.nvim_states.remove(&focused);

                                // Focus the first remaining pane.
                                let remaining = state.tab_layouts[active].all_pane_ids();
                                if let Some(&new_focus) = remaining.first() {
                                    state.set_focused_pane(new_focus);
                                }
                            }
                            state.selection.clear();
                            state.window.request_redraw();
                        }

                        KeyAction::ToggleZoom => {
                            let focused = focused_id;
                            let tab = state.tab_manager.active_tab_mut();
                            if tab.zoomed == Some(focused) {
                                tab.zoomed = None;
                            } else {
                                tab.zoomed = Some(focused);
                            }
                            state.window.request_redraw();
                        }

                        KeyAction::ResizePane(dir) => {
                            let focused = focused_id;
                            let delta = match dir {
                                Direction::Right | Direction::Down => RESIZE_DELTA,
                                Direction::Left | Direction::Up => -RESIZE_DELTA,
                            };
                            let active = state.tab_manager.active;
                            state.tab_layouts[active].resize_split(focused, delta);
                            state.window.request_redraw();
                        }

                        KeyAction::NewTab => {
                            // Compute size for a full-window single pane.
                            let win_size = state.window.inner_size();
                            let full_size = state.renderer.grid_size_for_window(
                                win_size.width,
                                win_size.height,
                                state.window.scale_factor(),
                            );
                            let new_id = state.spawn_pane(full_size);
                            let tab_idx = state.tab_manager.add_tab(new_id);
                            // Add the layout tree for the new tab.
                            state.tab_layouts.push(PaneNode::Leaf { pane_id: new_id });
                            state.tab_manager.switch_to(tab_idx);
                            state.set_focused_pane(new_id);
                            state.selection.clear();
                            state.window.request_redraw();
                        }

                        KeyAction::SwitchTab(n) => {
                            // n is 1-indexed.
                            state.tab_manager.switch_to(n.saturating_sub(1));
                            // Focus the active pane of the newly-switched-to tab.
                            let new_focus = state.tab_manager.active_tab().focus;
                            state.set_focused_pane(new_focus);
                            state.selection.clear();
                            state.window.request_redraw();
                        }

                        KeyAction::CloseTab => {
                            let active = state.tab_manager.active;
                            let removed_ids = state.tab_manager.close_tab(active);
                            if !removed_ids.is_empty() {
                                // Remove layout for this tab.
                                if active < state.tab_layouts.len() {
                                    state.tab_layouts.remove(active);
                                }
                                for id in removed_ids {
                                    let lid = id;
                                    state.panes.remove(&lid);
                                    state.pty_channels.remove(&lid);
                                }
                                // Focus active tab's pane.
                                let new_focus = state.tab_manager.active_tab().focus;
                                state.set_focused_pane(new_focus);
                                state.selection.clear();
                                state.window.request_redraw();
                            }
                        }

                        KeyAction::OpenPalette => {
                            state.palette_mode = Some(PaletteState::new());
                            log::info!("Command palette: open");
                            state.window.request_redraw();
                        }

                        KeyAction::Consumed => {
                            // Key consumed by state machine (leader chord entered).
                            // No PTY write needed.
                        }
                    }
                }
            }

            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: dispatch a KeyAction produced by the command palette.
//
// This covers the subset of KeyAction variants reachable from PaletteAction;
// Forward, SwitchTab, ResizePane, Consumed, and OpenPalette are never emitted
// by the palette and are handled as no-ops here.
// ---------------------------------------------------------------------------

fn execute_key_action(state: &mut AppState, event_loop: &ActiveEventLoop, action: KeyAction) {
    let focused_id = state.focused_pane();

    match action {
        KeyAction::NavigatePane(dir) => {
            let rects = state.compute_pane_rects();
            if let Some(new_focus) =
                state.active_layout().focus_in_direction(focused_id, dir, &rects)
            {
                state.set_focused_pane(new_focus);
                state.selection.clear();
            }
        }

        KeyAction::Split(axis) => {
            let rects = state.compute_pane_rects();
            let focused_rect = rects.get(&focused_id).copied().unwrap_or(PixelRect {
                x: 0.0,
                y: 0.0,
                width: 800.0,
                height: 600.0,
            });
            let new_rect = match axis {
                Axis::Horizontal => PixelRect { width: focused_rect.width / 2.0, ..focused_rect },
                Axis::Vertical => PixelRect { height: focused_rect.height / 2.0, ..focused_rect },
            };
            let new_size = state.grid_size_for_rect(new_rect);
            let new_id = state.spawn_pane(new_size);
            let orig_size = state.grid_size_for_rect(new_rect);
            if let Some(terminal) = state.panes.get_mut(&focused_id) {
                terminal.resize(orig_size);
            }
            let active = state.tab_manager.active;
            state.tab_layouts[active].split(focused_id, axis, new_id);
            state.set_focused_pane(new_id);
        }

        KeyAction::ClosePane => {
            let active = state.tab_manager.active;
            let pane_count = state.tab_layouts[active].all_pane_ids().len();
            if pane_count <= 1 {
                let removed_ids = state.tab_manager.close_tab(active);
                if active < state.tab_layouts.len() {
                    state.tab_layouts.remove(active);
                }
                for id in removed_ids {
                    let lid = id;
                    state.panes.remove(&lid);
                    state.pty_channels.remove(&lid);
                }
                if state.tab_manager.tab_count() == 0 {
                    event_loop.exit();
                    return;
                }
                let new_focus = state.tab_manager.active_tab().focus;
                state.set_focused_pane(new_focus);
            } else {
                let replacement = state.tab_layouts[active].close(focused_id);
                if let Some(new_root) = replacement {
                    state.tab_layouts[active] = new_root;
                }
                state.panes.remove(&focused_id);
                state.pty_channels.remove(&focused_id);
                let remaining = state.tab_layouts[active].all_pane_ids();
                if let Some(&new_focus) = remaining.first() {
                    state.set_focused_pane(new_focus);
                }
            }
            state.selection.clear();
        }

        KeyAction::ToggleZoom => {
            let tab = state.tab_manager.active_tab_mut();
            if tab.zoomed == Some(focused_id) {
                tab.zoomed = None;
            } else {
                tab.zoomed = Some(focused_id);
            }
        }

        KeyAction::NewTab => {
            let win_size = state.window.inner_size();
            let full_size = state.renderer.grid_size_for_window(
                win_size.width,
                win_size.height,
                state.window.scale_factor(),
            );
            let new_id = state.spawn_pane(full_size);
            let tab_idx = state.tab_manager.add_tab(new_id);
            state.tab_layouts.push(PaneNode::Leaf { pane_id: new_id });
            state.tab_manager.switch_to(tab_idx);
            state.set_focused_pane(new_id);
            state.selection.clear();
        }

        KeyAction::CloseTab => {
            let active = state.tab_manager.active;
            let removed_ids = state.tab_manager.close_tab(active);
            if !removed_ids.is_empty() {
                if active < state.tab_layouts.len() {
                    state.tab_layouts.remove(active);
                }
                for id in removed_ids {
                    let lid = id;
                    state.panes.remove(&lid);
                    state.pty_channels.remove(&lid);
                }
                let new_focus = state.tab_manager.active_tab().focus;
                state.set_focused_pane(new_focus);
                state.selection.clear();
            }
        }

        // These are not reachable from palette actions but must be exhaustive.
        KeyAction::Forward(_)
        | KeyAction::ResizePane(_)
        | KeyAction::SwitchTab(_)
        | KeyAction::OpenPalette
        | KeyAction::Consumed => {}
    }
}

// ---------------------------------------------------------------------------
// Helper: convert a physical pixel position to a grid CellPos relative to a
// pane's origin rect.
// ---------------------------------------------------------------------------

fn cursor_to_cell_in_rect(
    state: &AppState,
    px: f64,
    py: f64,
    pane_rect: PixelRect,
) -> selection::CellPos {
    let scale = state.window.scale_factor();
    let cell_w = state.renderer.text.cell_size.width as f64;
    let cell_h = state.renderer.text.cell_size.height as f64;
    // Convert pixel position relative to pane origin.
    let rel_x = px - pane_rect.x as f64;
    let rel_y = py - pane_rect.y as f64;
    pixel_to_cell(rel_x, rel_y, cell_w, cell_h, scale)
}

// ---------------------------------------------------------------------------
// Helper: resolve a RenderPalette from the loaded configuration.
// ---------------------------------------------------------------------------

/// Build a [`RenderPalette`] from an [`ArctermConfig`].
fn palette_from_config(cfg: &config::ArctermConfig) -> RenderPalette {
    let app_palette = colors::ColorPalette::by_name(&cfg.color_scheme)
        .unwrap_or_else(|| {
            log::warn!(
                "config: unknown color_scheme {:?}, falling back to catppuccin-mocha",
                cfg.color_scheme
            );
            colors::ColorPalette::default()
        })
        .with_overrides(&cfg.colors);

    RenderPalette {
        foreground: app_palette.foreground,
        background: app_palette.background,
        cursor:     app_palette.cursor,
        ansi:       app_palette.ansi,
    }
}
