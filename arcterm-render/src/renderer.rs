//! High-level Renderer: combines GpuState + QuadRenderer + TextRenderer.

use std::sync::Arc;

use arcterm_core::{Color as TermColor, Grid, GridSize};
use winit::window::Window;

use crate::gpu::GpuState;
use crate::quad::{QuadInstance, QuadRenderer};
use crate::text::{TextRenderer, ansi_color_to_glyphon};

/// Default font size (logical pixels / points).
const FONT_SIZE: f32 = 14.0;

/// Dark background color (30, 30, 46 in 0–1 range) — used for the clear load op.
const BG_COLOR: wgpu::Color = wgpu::Color {
    r: 30.0 / 255.0,
    g: 30.0 / 255.0,
    b: 46.0 / 255.0,
    a: 1.0,
};

/// Default terminal background as an RGBA f32 array (matches BG_COLOR).
const BG_COLOR_F32: [f32; 4] = [30.0 / 255.0, 30.0 / 255.0, 46.0 / 255.0, 1.0];

/// Cursor block color (soft white).
const CURSOR_COLOR: [f32; 4] = [0.85, 0.85, 0.85, 1.0];

/// Top-level renderer: owns GPU state, quad pipeline, and text renderer.
pub struct Renderer {
    pub gpu: GpuState,
    pub text: TextRenderer,
    pub quads: QuadRenderer,
}

impl Renderer {
    /// Create the renderer, initializing wgpu, the quad pipeline, and glyphon.
    pub fn new(window: Arc<Window>) -> Self {
        let gpu = GpuState::new(window);
        let text = TextRenderer::new(
            &gpu.device,
            &gpu.queue,
            gpu.surface_format,
            FONT_SIZE,
        );
        let quads = QuadRenderer::new(&gpu.device, gpu.surface_format);
        Self { gpu, text, quads }
    }

    /// Handle window resize: reconfigure the surface and clear row hashes.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.gpu.resize(width, height);
        // Clear row hashes so all rows are re-shaped after a resize.
        self.text.row_hashes.clear();
    }

    /// Render a full terminal grid frame.
    pub fn render_frame(&mut self, grid: &Grid, scale_factor: f64) {
        let sf = scale_factor as f32;
        let w = self.gpu.surface_config.width;
        let h = self.gpu.surface_config.height;

        self.text.update_viewport(&self.gpu.queue, w, h);

        // Build quad instances for non-default cell backgrounds and cursor.
        let quad_instances = build_quad_instances(grid, &self.text.cell_size, sf);

        // Upload quads to GPU.
        self.quads.prepare(&self.gpu.queue, &quad_instances, w, h);

        // Prepare text — suppress errors on atlas full (rare on first frame).
        let _ = self.text.prepare_grid(&self.gpu.device, &self.gpu.queue, grid, sf);

        let (frame, view) = match self.gpu.begin_frame() {
            Ok(pair) => pair,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                // Surface was lost (common on macOS during occlusion/restoration).
                // Reconfigure with current dimensions and skip this frame.
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
                        load: wgpu::LoadOp::Clear(BG_COLOR),
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
// Quad instance builder
// ---------------------------------------------------------------------------

/// Convert the grid into a list of QuadInstances for non-default backgrounds
/// and the cursor block.
fn build_quad_instances(
    grid: &Grid,
    cell_size: &crate::text::CellSize,
    scale_factor: f32,
) -> Vec<QuadInstance> {
    let rows = grid.rows_for_viewport();
    let cursor = grid.cursor;
    let cursor_visible = grid.modes.cursor_visible;
    let cell_w = cell_size.width * scale_factor;
    let cell_h = cell_size.height * scale_factor;

    let mut quads: Vec<QuadInstance> = Vec::new();

    for (row_idx, row) in rows.iter().enumerate() {
        let y = row_idx as f32 * cell_h;
        for (col_idx, cell) in row.iter().enumerate() {
            let x = col_idx as f32 * cell_w;
            let is_cursor = cursor_visible
                && row_idx == cursor.row
                && col_idx == cursor.col;

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
                    color: term_color_to_f32(eff_bg, false),
                });
            }

            // Cursor block: draw on top of (possibly colored) background.
            // The text renderer draws the glyph at this cell in the background
            // color (via the reverse path or the default bg) so it is readable.
            if is_cursor {
                // Use the cursor color unless the cell has a custom fg that
                // would serve better as the block color.
                let block_color = if matches!(eff_fg, TermColor::Default) {
                    CURSOR_COLOR
                } else {
                    term_color_to_f32(eff_fg, true)
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
fn term_color_to_f32(color: TermColor, is_fg: bool) -> [f32; 4] {
    let g = ansi_color_to_glyphon(color, is_fg);
    // glyphon::Color stores components as u8.
    [
        g.r() as f32 / 255.0,
        g.g() as f32 / 255.0,
        g.b() as f32 / 255.0,
        1.0,
    ]
}

// Suppress warning — BG_COLOR_F32 is available for future use.
const _: [f32; 4] = BG_COLOR_F32;
