//! High-level Renderer: combines GpuState + QuadRenderer + TextRenderer.

use std::sync::Arc;

use arcterm_core::{Color as TermColor, Grid, GridSize};
use winit::window::Window;

use crate::gpu::GpuState;
use crate::palette::RenderPalette;
use crate::quad::{QuadInstance, QuadRenderer};
use crate::text::{ClipRect, TextRenderer, ansi_color_to_glyphon};

// ---------------------------------------------------------------------------
// Multi-pane types
// ---------------------------------------------------------------------------

/// Describes a single pane to render in a multi-pane frame.
pub struct PaneRenderInfo<'a> {
    /// The terminal grid to render.
    pub grid: &'a Grid,
    /// Bounding rectangle in physical pixels: [x, y, width, height].
    pub rect: [f32; 4],
}

/// A solid-color overlay quad (used for borders, tab bar backgrounds, etc.).
#[derive(Clone, Copy, Debug)]
pub struct OverlayQuad {
    /// Bounding rectangle in physical pixels: [x, y, width, height].
    pub rect: [f32; 4],
    /// RGBA color, components in [0, 1].
    pub color: [f32; 4],
}

// ---------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------

/// Top-level renderer: owns GPU state, quad pipeline, text renderer, and
/// the active colour palette.
pub struct Renderer {
    pub gpu: GpuState,
    pub text: TextRenderer,
    pub quads: QuadRenderer,
    /// Active colour palette — hot-reloadable via [`set_palette`].
    pub palette: RenderPalette,
}

impl Renderer {
    /// Create the renderer, initializing wgpu, the quad pipeline, and glyphon.
    ///
    /// `font_size` is in logical pixels / points.  Pass `FONT_SIZE` (14.0) to
    /// use the compiled-in default, or a value from configuration to honour the
    /// user's preference.
    pub fn new(window: Arc<Window>, font_size: f32) -> Self {
        let gpu = GpuState::new(window);
        let text = TextRenderer::new(
            &gpu.device,
            &gpu.queue,
            gpu.surface_format,
            font_size,
        );
        let quads = QuadRenderer::new(&gpu.device, gpu.surface_format);
        Self {
            gpu,
            text,
            quads,
            palette: RenderPalette::default(),
        }
    }

    /// Replace the active colour palette.
    ///
    /// Takes effect on the next call to [`render_frame`].  All row hashes are
    /// cleared so every row is re-shaped with the new colours on the next frame.
    pub fn set_palette(&mut self, palette: RenderPalette) {
        self.palette = palette;
        // Force full re-shape so text picks up the new palette colours.
        self.text.row_hashes.clear();
    }

    /// Handle window resize: reconfigure the surface and clear row hashes.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.gpu.resize(width, height);
        // Clear row hashes so all rows are re-shaped after a resize.
        self.text.row_hashes.clear();
    }

    /// Render a full terminal grid frame.
    ///
    /// Delegates to [`render_multipane`] with a single pane that fills the
    /// entire surface.
    pub fn render_frame(&mut self, grid: &Grid, scale_factor: f64) {
        let w = self.gpu.surface_config.width as f32;
        let h = self.gpu.surface_config.height as f32;

        let pane = PaneRenderInfo {
            grid,
            rect: [0.0, 0.0, w, h],
        };
        self.render_multipane(&[pane], &[], scale_factor);
    }

    /// Render multiple panes and overlay quads in a single GPU pass.
    ///
    /// For each pane:
    /// - Cell background quads and cursor blocks are built with `build_quad_instances_at`.
    /// - Text is shaped via `prepare_grid_at` and submitted all at once.
    ///
    /// `overlay_quads` are drawn on top of all cell backgrounds but beneath
    /// text (e.g. borders, tab bar backgrounds).
    pub fn render_multipane(
        &mut self,
        panes: &[PaneRenderInfo<'_>],
        overlay_quads: &[OverlayQuad],
        scale_factor: f64,
    ) {
        let sf = scale_factor as f32;
        let w = self.gpu.surface_config.width;
        let h = self.gpu.surface_config.height;

        self.text.update_viewport(&self.gpu.queue, w, h);
        self.text.reset_frame();

        // Build all quad instances: cell backgrounds + cursor per pane, then overlays.
        let mut all_quads: Vec<QuadInstance> = Vec::new();

        for pane in panes {
            let [px, py, _pw, _ph] = pane.rect;
            let clip = ClipRect {
                x: px as i32,
                y: py as i32,
                width: pane.rect[2] as u32,
                height: pane.rect[3] as u32,
            };

            let pane_quads = build_quad_instances_at(
                pane.grid,
                &self.text.cell_size,
                sf,
                &self.palette,
                px,
                py,
            );
            all_quads.extend_from_slice(&pane_quads);

            self.text.prepare_grid_at(
                pane.grid,
                px,
                py,
                Some(clip),
                sf,
                &self.palette,
            );
        }

        // Append overlay quads (borders, tab bar backgrounds, etc.).
        for oq in overlay_quads {
            all_quads.push(QuadInstance {
                rect: oq.rect,
                color: oq.color,
            });
        }

        // Upload quads to GPU.
        self.quads.prepare(&self.gpu.queue, &all_quads, w, h);

        // Upload text to GPU.
        let _ = self.text.submit_text_areas(&self.gpu.device, &self.gpu.queue);

        let (frame, view) = match self.gpu.begin_frame() {
            Ok(pair) => pair,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.gpu.resize(w, h);
                return;
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                log::error!("GPU out of memory — skipping frame");
                return;
            }
            Err(e) => {
                log::warn!("begin_frame error: {e:?} — skipping frame");
                return;
            }
        };

        let mut encoder =
            self.gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("arcterm frame encoder"),
                });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("arcterm render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.palette.bg_wgpu()),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // 1. Draw cell background quads (and cursor block) first.
            self.quads.render(&mut pass);

            // 2. Draw text glyphs on top.
            let _ = self.text.render(&mut pass);
        }

        self.gpu.queue.submit(Some(encoder.finish()));
        frame.present();
    }

    /// Calculate how many columns × rows fit the window at the given scale.
    pub fn grid_size_for_window(&self, width: u32, height: u32, scale_factor: f64) -> GridSize {
        let sf = scale_factor as f32;
        let cell_w = self.text.cell_size.width * sf;
        let cell_h = self.text.cell_size.height * sf;
        if cell_w <= 0.0 || cell_h <= 0.0 {
            return GridSize::new(24, 80);
        }
        let cols = ((width as f32) / cell_w).floor() as usize;
        let rows = ((height as f32) / cell_h).floor() as usize;
        GridSize::new(rows.max(1), cols.max(1))
    }
}

// ---------------------------------------------------------------------------
// Quad instance builders
// ---------------------------------------------------------------------------

/// Convert the grid into a list of QuadInstances for non-default backgrounds
/// and the cursor block, offset to a specific pane origin.
pub fn build_quad_instances_at(
    grid: &Grid,
    cell_size: &crate::text::CellSize,
    scale_factor: f32,
    palette: &RenderPalette,
    offset_x: f32,
    offset_y: f32,
) -> Vec<QuadInstance> {
    let rows = grid.rows_for_viewport();
    let cursor = grid.cursor;
    let cursor_visible = grid.modes.cursor_visible;
    let cell_w = cell_size.width * scale_factor;
    let cell_h = cell_size.height * scale_factor;

    let mut quads: Vec<QuadInstance> = Vec::new();

    for (row_idx, row) in rows.iter().enumerate() {
        let y = offset_y + row_idx as f32 * cell_h;
        for (col_idx, cell) in row.iter().enumerate() {
            let x = offset_x + col_idx as f32 * cell_w;
            let is_cursor = cursor_visible && row_idx == cursor.row && col_idx == cursor.col;

            // Determine effective fg/bg (swapped for reverse attribute).
            let (eff_fg, eff_bg) = if cell.attrs.reverse {
                (cell.attrs.bg, cell.attrs.fg)
            } else {
                (cell.attrs.fg, cell.attrs.bg)
            };

            // Emit a background quad for non-default background colors.
            let bg_is_default = matches!(eff_bg, TermColor::Default);
            if !bg_is_default {
                quads.push(QuadInstance {
                    rect: [x, y, cell_w, cell_h],
                    color: term_color_to_f32(eff_bg, false, palette),
                });
            }

            // Cursor block: draw on top of (possibly colored) background.
            if is_cursor {
                let block_color = if matches!(eff_fg, TermColor::Default) {
                    palette.cursor_f32()
                } else {
                    term_color_to_f32(eff_fg, true, palette)
                };
                quads.push(QuadInstance {
                    rect: [x, y, cell_w, cell_h],
                    color: block_color,
                });
            }
        }
    }

    quads
}

/// Convert a terminal Color to an RGBA f32 array.
fn term_color_to_f32(color: TermColor, is_fg: bool, palette: &RenderPalette) -> [f32; 4] {
    let g = ansi_color_to_glyphon(color, is_fg, palette);
    // glyphon::Color stores components as u8.
    [
        g.r() as f32 / 255.0,
        g.g() as f32 / 255.0,
        g.b() as f32 / 255.0,
        1.0,
    ]
}

// ---------------------------------------------------------------------------
// Tab bar helpers
// ---------------------------------------------------------------------------

/// Compute the height in physical pixels of the tab bar.
///
/// The tab bar is one cell tall plus a small vertical padding.
pub fn tab_bar_height(cell_size: &crate::text::CellSize, scale_factor: f32) -> f32 {
    cell_size.height * scale_factor * 1.2
}

/// Build `QuadInstance`s for a tab bar.
///
/// Returns one background quad per tab.  The active tab index is highlighted
/// using the palette cursor color; inactive tabs use the palette background
/// darkened (expressed as a fixed overlay).
///
/// `tabs` — list of tab labels (length determines number of tabs).
/// `active_idx` — which tab is currently active.
/// `cell_size` — logical cell dimensions.
/// `scale_factor` — physical pixels per logical pixel.
/// `window_width` — full window width in physical pixels.
/// `palette` — active colour palette.
pub fn render_tab_bar_quads(
    tab_count: usize,
    active_idx: usize,
    cell_size: &crate::text::CellSize,
    scale_factor: f32,
    window_width: f32,
    palette: &RenderPalette,
) -> Vec<QuadInstance> {
    if tab_count == 0 {
        return Vec::new();
    }

    let bar_h = tab_bar_height(cell_size, scale_factor);
    let tab_w = window_width / tab_count as f32;

    let (br, bg_b, bb) = palette.background;
    let inactive_color: [f32; 4] = [
        (br as f32 / 255.0) * 1.3_f32.min(1.0),
        (bg_b as f32 / 255.0) * 1.3_f32.min(1.0),
        (bb as f32 / 255.0) * 1.3_f32.min(1.0),
        1.0,
    ];
    let active_color = palette.cursor_f32();

    let mut quads = Vec::with_capacity(tab_count);
    for i in 0..tab_count {
        let color = if i == active_idx {
            active_color
        } else {
            inactive_color
        };
        quads.push(QuadInstance {
            rect: [i as f32 * tab_w, 0.0, tab_w, bar_h],
            color,
        });
    }
    quads
}
