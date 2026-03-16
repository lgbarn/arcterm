//! Text rendering using glyphon (cosmic-text + wgpu atlas).

use crate::snapshot::{RenderSnapshot, SnapshotCell, SnapshotColor};
use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, Style, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer as GlyphonTextRenderer, Viewport, Weight,
};
use wgpu::MultisampleState;

use crate::palette::RenderPalette;
use crate::structured::RenderedLine;

/// Cell dimensions measured from the font metrics.
#[derive(Clone, Copy, Debug)]
pub struct CellSize {
    pub width: f32,
    pub height: f32,
}

/// A plugin-rendered styled line, as produced by the WASM `render()` export
/// and translated by the caller before passing to `TextRenderer::prepare_plugin_pane`.
///
/// This type lives in arcterm-render so that the renderer does not need to
/// import arcterm-plugin (which would introduce a circular dependency via
/// arcterm-app).
#[derive(Clone, Debug, Default)]
pub struct PluginStyledLine {
    pub text: String,
    /// Foreground RGB colour, or `None` to use the default palette foreground.
    pub fg: Option<(u8, u8, u8)>,
    /// Background RGB colour (currently unused in rendering, reserved for future).
    pub bg: Option<(u8, u8, u8)>,
    pub bold: bool,
    pub italic: bool,
}

/// Clip rectangle for text rendering.
#[derive(Clone, Copy, Debug)]
pub struct ClipRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Per-pane metadata for multi-pane text submission.
struct PaneSlot {
    /// Pixel offset for this grid.
    offset_x: f32,
    offset_y: f32,
    /// Optional clip region (in physical pixels).
    clip: Option<ClipRect>,
    /// Number of rows shaped for this pane.
    num_rows: usize,
    /// Scale factor used when shaping.
    scale_factor: f32,
    /// Default foreground colour for this pane.
    default_fg: Color,
}

/// Manages all glyphon state and converts a terminal Grid to GPU text draws.
pub struct TextRenderer {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub viewport: Viewport,
    pub atlas: TextAtlas,
    pub renderer: GlyphonTextRenderer,
    /// Per-row Buffers for the single-pane path; resized as needed.
    row_buffers: Vec<Buffer>,
    pub cell_size: CellSize,
    /// Font size in logical pixels.
    font_size: f32,
    line_height: f32,
    /// Per-row content hashes for dirty-row optimization.
    pub row_hashes: Vec<u64>,

    // ---- Multi-pane accumulation ----------------------------------------
    /// Pool of row-buffer vectors, one per accumulated pane.
    pane_buffer_pool: Vec<Vec<Buffer>>,
    /// Metadata for each accumulated pane (parallel to `pane_buffer_pool`).
    pane_slots: Vec<PaneSlot>,
}

impl TextRenderer {
    /// Construct and measure cell dimensions from a monospace font.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        font_size: f32,
    ) -> Self {
        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, surface_format);
        let renderer =
            GlyphonTextRenderer::new(&mut atlas, device, MultisampleState::default(), None);

        let line_height = font_size * 1.4;
        let cell_size = measure_cell(&mut font_system, font_size, line_height);

        Self {
            font_system,
            swash_cache,
            viewport,
            atlas,
            renderer,
            row_buffers: Vec::new(),
            cell_size,
            font_size,
            line_height,
            row_hashes: Vec::new(),
            pane_buffer_pool: Vec::new(),
            pane_slots: Vec::new(),
        }
    }

    /// Update the viewport resolution (call before prepare each frame).
    pub fn update_viewport(&mut self, queue: &wgpu::Queue, width: u32, height: u32) {
        self.viewport.update(queue, Resolution { width, height });
    }

    /// Reset per-frame multi-pane accumulation state.
    ///
    /// Call once at the start of each frame before any `prepare_grid_at` calls.
    /// This clears the pane slot list while keeping allocated `Buffer` memory
    /// in the pool for reuse.
    pub fn reset_frame(&mut self) {
        // Move pane buffers back into the pool (swap out and drain slots).
        // pane_buffer_pool already holds them; just clear the metadata.
        self.pane_slots.clear();
        // Truncate pool to match (it's grown cumulatively; clear overshoot).
        self.pane_buffer_pool.truncate(0);
    }

    /// Convert a `RenderSnapshot` into glyphon TextAreas and upload to the atlas.
    ///
    /// The cursor row is always re-shaped; other rows are skipped when their
    /// content hash is unchanged (dirty-row optimization).
    pub fn prepare_grid(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        snapshot: &RenderSnapshot,
        scale_factor: f32,
        palette: &RenderPalette,
    ) -> Result<(), glyphon::PrepareError> {
        let num_rows = snapshot.rows;
        let cell_w = self.cell_size.width;
        let cell_h = self.cell_size.height;

        // Grow or shrink the row buffer pool.
        while self.row_buffers.len() < num_rows {
            let buf =
                Buffer::new(&mut self.font_system, Metrics::new(self.font_size, self.line_height));
            self.row_buffers.push(buf);
        }
        self.row_buffers.truncate(num_rows);

        // Sync hash vec length with row count (resize clears stale hashes).
        self.row_hashes.resize(num_rows, u64::MAX);

        // Populate each row buffer with per-cell colored spans.
        for row_idx in 0..num_rows {
            let row = snapshot.row(row_idx);
            let is_cursor_row = row_idx == snapshot.cursor_row;

            // Compute a cheap content hash for dirty-row skipping.
            let cursor_col_opt = if is_cursor_row { Some(snapshot.cursor_col) } else { None };
            let row_hash = hash_row(row, row_idx, cursor_col_opt);

            if !is_cursor_row && self.row_hashes[row_idx] == row_hash {
                // Row is unchanged — reuse existing Buffer, skip re-shaping.
                continue;
            }
            self.row_hashes[row_idx] = row_hash;

            let buf = &mut self.row_buffers[row_idx];
            let width_px = cell_w * row.len() as f32;
            buf.set_size(
                &mut self.font_system,
                Some(width_px * scale_factor),
                Some(cell_h * scale_factor),
            );

            shape_row_into_buffer(buf, row, &mut self.font_system, palette, cursor_col_opt);
        }

        // Build TextArea slice from the now-stable row_buffers.
        let default_fg = palette.fg_glyphon();
        let scaled_cell_h = cell_h * scale_factor;
        let text_areas: Vec<TextArea> = self
            .row_buffers
            .iter()
            .enumerate()
            .map(|(row_idx, buf)| TextArea {
                buffer: buf,
                left: 0.0,
                top: row_idx as f32 * scaled_cell_h,
                scale: scale_factor,
                bounds: TextBounds::default(),
                default_color: default_fg,
                custom_glyphs: &[],
            })
            .collect();

        self.renderer.prepare(
            device,
            queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            text_areas,
            &mut self.swash_cache,
        )
    }

    /// Shape a `RenderSnapshot` at a given pixel offset and optional clip,
    /// accumulating it for a later [`submit_text_areas`] call.
    ///
    /// This is the multi-pane variant of [`prepare_grid`].  Call
    /// [`reset_frame`] at the start of each frame, then call this method once
    /// per pane, then call [`submit_text_areas`] to upload everything to the
    /// GPU in a single `renderer.prepare` invocation.
    ///
    /// `offset_x` / `offset_y` are in physical pixels (scale_factor already
    /// applied by the caller).
    pub fn prepare_grid_at(
        &mut self,
        snapshot: &RenderSnapshot,
        offset_x: f32,
        offset_y: f32,
        clip: Option<ClipRect>,
        scale_factor: f32,
        palette: &RenderPalette,
    ) {
        let num_rows = snapshot.rows;
        let cell_w = self.cell_size.width;
        let cell_h = self.cell_size.height;

        // Allocate or reuse a buffer vector for this slot.
        let slot_idx = self.pane_slots.len();
        if slot_idx >= self.pane_buffer_pool.len() {
            self.pane_buffer_pool.push(Vec::new());
        }
        let buf_vec = &mut self.pane_buffer_pool[slot_idx];

        // Grow the buffer vector as needed.
        while buf_vec.len() < num_rows {
            let b = Buffer::new(
                &mut self.font_system,
                Metrics::new(self.font_size, self.line_height),
            );
            buf_vec.push(b);
        }
        buf_vec.truncate(num_rows);

        // Shape each row.
        for (row_idx, buf) in buf_vec.iter_mut().enumerate() {
            let row = snapshot.row(row_idx);
            let width_px = cell_w * row.len() as f32;
            buf.set_size(
                &mut self.font_system,
                Some(width_px * scale_factor),
                Some(cell_h * scale_factor),
            );
            let cursor_col = if row_idx == snapshot.cursor_row { Some(snapshot.cursor_col) } else { None };
            shape_row_into_buffer(buf, row, &mut self.font_system, palette, cursor_col);
        }

        self.pane_slots.push(PaneSlot {
            offset_x,
            offset_y,
            clip,
            num_rows,
            scale_factor,
            default_fg: palette.fg_glyphon(),
        });
    }

    /// Submit all panes accumulated since the last [`reset_frame`] to the GPU.
    ///
    /// Builds one `TextArea` per row across all panes and calls
    /// `renderer.prepare` once.  Returns the glyphon `PrepareError` if the
    /// atlas is full.
    pub fn submit_text_areas(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(), glyphon::PrepareError> {
        let cell_h = self.cell_size.height;

        // We need to build TextArea slices borrowing from pane_buffer_pool.
        // Collect all areas in one pass.
        let mut text_areas: Vec<TextArea> = Vec::new();

        for (slot, meta) in self.pane_buffer_pool.iter().zip(self.pane_slots.iter()) {
            let bounds = if let Some(clip) = meta.clip {
                TextBounds {
                    left: clip.x,
                    top: clip.y,
                    right: clip.x + clip.width as i32,
                    bottom: clip.y + clip.height as i32,
                }
            } else {
                TextBounds::default()
            };

            let scaled_cell_h = cell_h * meta.scale_factor;
            for (row_idx, buf) in slot.iter().take(meta.num_rows).enumerate() {
                text_areas.push(TextArea {
                    buffer: buf,
                    left: meta.offset_x,
                    top: meta.offset_y + row_idx as f32 * scaled_cell_h,
                    scale: meta.scale_factor,
                    bounds,
                    default_color: meta.default_fg,
                    custom_glyphs: &[],
                });
            }
        }

        self.renderer.prepare(
            device,
            queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            text_areas,
            &mut self.swash_cache,
        )
    }

    /// Prepare tab bar text labels, appending them to the multi-pane accumulator.
    ///
    /// Each label is positioned according to `positions` (physical-pixel x
    /// offsets) at a fixed `y` offset.  `clip` optionally clips to the tab-bar
    /// area.
    pub fn prepare_tab_bar_text(
        &mut self,
        labels: &[String],
        positions_x: &[f32],
        y: f32,
        clip: Option<ClipRect>,
        scale_factor: f32,
        palette: &RenderPalette,
    ) {
        let cell_h = self.cell_size.height;

        for (label, &x) in labels.iter().zip(positions_x.iter()) {
            let slot_idx = self.pane_slots.len();
            if slot_idx >= self.pane_buffer_pool.len() {
                self.pane_buffer_pool.push(Vec::new());
            }
            let buf_vec = &mut self.pane_buffer_pool[slot_idx];
            if buf_vec.is_empty() {
                buf_vec.push(Buffer::new(
                    &mut self.font_system,
                    Metrics::new(self.font_size, self.line_height),
                ));
            }

            let buf = &mut buf_vec[0];
            buf.set_size(
                &mut self.font_system,
                Some(500.0 * scale_factor),
                Some(cell_h * scale_factor),
            );
            buf.set_text(
                &mut self.font_system,
                label.as_str(),
                &Attrs::new()
                    .family(Family::Monospace)
                    .color(palette.fg_glyphon()),
                Shaping::Basic,
                None,
            );
            buf.shape_until_scroll(&mut self.font_system, false);

            self.pane_slots.push(PaneSlot {
                offset_x: x,
                offset_y: y,
                clip,
                num_rows: 1,
                scale_factor,
                default_fg: palette.fg_glyphon(),
            });
        }
    }

    /// Prepare arbitrary overlay text labels at absolute pixel positions.
    ///
    /// Each entry is `(text, x, y)` in physical pixels.  Used for the command
    /// palette overlay.  Call this after `prepare_grid_at` and before
    /// `submit_text_areas`.
    pub fn prepare_overlay_text(
        &mut self,
        entries: &[(String, f32, f32)],
        scale_factor: f32,
        fg: glyphon::Color,
    ) {
        let cell_h = self.cell_size.height;

        for (text, x, y) in entries {
            let slot_idx = self.pane_slots.len();
            if slot_idx >= self.pane_buffer_pool.len() {
                self.pane_buffer_pool.push(Vec::new());
            }
            let buf_vec = &mut self.pane_buffer_pool[slot_idx];
            if buf_vec.is_empty() {
                buf_vec.push(Buffer::new(
                    &mut self.font_system,
                    Metrics::new(self.font_size, self.line_height),
                ));
            }

            let buf = &mut buf_vec[0];
            buf.set_size(
                &mut self.font_system,
                Some(2000.0 * scale_factor),
                Some(cell_h * scale_factor),
            );
            buf.set_text(
                &mut self.font_system,
                text.as_str(),
                &Attrs::new().family(Family::Monospace).color(fg),
                Shaping::Basic,
                None,
            );
            buf.shape_until_scroll(&mut self.font_system, false);

            self.pane_slots.push(PaneSlot {
                offset_x: *x,
                offset_y: *y,
                clip: None,
                num_rows: 1,
                scale_factor,
                default_fg: fg,
            });
        }
    }

    /// Shape a rendered structured block into the multi-pane accumulator.
    ///
    /// Each `RenderedLine` in the block becomes one glyphon `Buffer` positioned
    /// at `(offset_x, offset_y + line_idx * cell_h)`.  Rich-text styling is
    /// applied using the `StyledSpan` color, bold, and italic attributes.
    ///
    /// Call this after `prepare_grid_at` for the same pane and before
    /// `submit_text_areas`.
    pub fn prepare_structured_block(
        &mut self,
        lines: &[RenderedLine],
        offset_x: f32,
        offset_y: f32,
        clip: Option<ClipRect>,
        scale_factor: f32,
    ) {
        let cell_h = self.cell_size.height;

        for (line_idx, rendered_line) in lines.iter().enumerate() {
            if rendered_line.spans.is_empty() {
                continue;
            }

            let slot_idx = self.pane_slots.len();
            if slot_idx >= self.pane_buffer_pool.len() {
                self.pane_buffer_pool.push(Vec::new());
            }
            let buf_vec = &mut self.pane_buffer_pool[slot_idx];
            if buf_vec.is_empty() {
                buf_vec.push(Buffer::new(
                    &mut self.font_system,
                    Metrics::new(self.font_size, self.line_height),
                ));
            }

            let buf = &mut buf_vec[0];
            buf.set_size(
                &mut self.font_system,
                Some(8000.0 * scale_factor),
                Some(cell_h * scale_factor),
            );

            // Build rich-text spans from StyledSpan attributes.
            let span_data: Vec<(String, Attrs)> = rendered_line
                .spans
                .iter()
                .map(|span| {
                    let color = Color::rgb(span.color.0, span.color.1, span.color.2);
                    let mut attrs = Attrs::new().family(Family::Monospace).color(color);
                    if span.bold {
                        attrs = attrs.weight(Weight::BOLD);
                    }
                    if span.italic {
                        attrs = attrs.style(Style::Italic);
                    }
                    (span.text.clone(), attrs)
                })
                .collect();

            buf.set_rich_text(
                &mut self.font_system,
                span_data.iter().map(|(s, a)| (s.as_str(), a.clone())),
                &Attrs::new().family(Family::Monospace),
                Shaping::Basic,
                None,
            );
            buf.shape_until_scroll(&mut self.font_system, false);

            let line_y = offset_y + line_idx as f32 * cell_h;
            self.pane_slots.push(PaneSlot {
                offset_x,
                offset_y: line_y,
                clip,
                num_rows: 1,
                scale_factor,
                default_fg: Color::rgb(200, 200, 200),
            });
        }
    }

    /// Prepare plugin pane output for rendering.
    ///
    /// Takes a slice of `PluginStyledLine` values (translated from the WIT
    /// `StyledLine` records produced by a plugin's `render()` export) and
    /// appends them to the multi-pane accumulator positioned within `pane_rect`.
    ///
    /// Each line is placed at `(pane_rect.x, pane_rect.y + line_idx * cell_h)`
    /// in physical pixels.  The clip region is set to the pane rect so lines
    /// cannot bleed into adjacent panes.
    ///
    /// Call after `reset_frame()` and before `submit_text_areas()`.
    pub fn prepare_plugin_pane(
        &mut self,
        pane_rect: &[f32; 4], // [x, y, width, height] in physical pixels
        lines: &[PluginStyledLine],
        scale_factor: f32,
    ) {
        use glyphon::{Attrs, Weight, Style as GlyphonStyle};

        let cell_h = self.cell_size.height;
        let [rect_x, rect_y, rect_w, rect_h] = *pane_rect;

        let clip = ClipRect {
            x: rect_x as i32,
            y: rect_y as i32,
            width: rect_w as u32,
            height: rect_h as u32,
        };

        for (line_idx, line) in lines.iter().enumerate() {
            if line.text.is_empty() {
                continue;
            }

            let slot_idx = self.pane_slots.len();
            if slot_idx >= self.pane_buffer_pool.len() {
                self.pane_buffer_pool.push(Vec::new());
            }
            let buf_vec = &mut self.pane_buffer_pool[slot_idx];
            if buf_vec.is_empty() {
                buf_vec.push(Buffer::new(
                    &mut self.font_system,
                    Metrics::new(self.font_size, self.line_height),
                ));
            }

            let buf = &mut buf_vec[0];
            buf.set_size(
                &mut self.font_system,
                Some(rect_w * scale_factor),
                Some(cell_h * scale_factor),
            );

            // Build glyph attrs from the StyledLine attributes.
            let fg_color = line
                .fg
                .map(|(r, g, b)| Color::rgb(r, g, b))
                .unwrap_or(Color::rgb(200, 200, 200));

            let mut attrs = Attrs::new()
                .family(Family::Monospace)
                .color(fg_color);

            if line.bold {
                attrs = attrs.weight(Weight::BOLD);
            }
            if line.italic {
                attrs = attrs.style(GlyphonStyle::Italic);
            }

            buf.set_text(
                &mut self.font_system,
                line.text.as_str(),
                &attrs,
                Shaping::Basic,
                None,
            );
            buf.shape_until_scroll(&mut self.font_system, false);

            let line_y = rect_y + line_idx as f32 * cell_h;

            self.pane_slots.push(PaneSlot {
                offset_x: rect_x,
                offset_y: line_y,
                clip: Some(clip),
                num_rows: 1,
                scale_factor,
                default_fg: fg_color,
            });
        }
    }

    /// Render previously prepared text into the active render pass, then trim
    /// the glyph atlas to reclaim unused cache entries.
    ///
    /// The atlas trim is performed inside this method so callers cannot forget
    /// to invoke it — omitting it would cause unbounded atlas growth.
    pub fn render<'pass>(
        &'pass mut self,
        pass: &mut wgpu::RenderPass<'pass>,
    ) -> Result<(), glyphon::RenderError> {
        let result = self.renderer.render(&self.atlas, &self.viewport, pass);
        self.atlas.trim();
        result
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return the effective character for each cell in a row, substituting
/// U+2588 (FULL BLOCK) at the cursor column when the cell is blank.
///
/// This is a pure function used by [`shape_row_into_buffer`] and tested
/// independently of the GPU font pipeline.
///
/// `cursor_col` — `Some(col)` when this is the cursor row, `None` otherwise.
/// A blank cell is one whose character is `' '` (space) or `'\0'` (null).
pub(crate) fn substitute_cursor_char(
    row: &[SnapshotCell],
    cursor_col: Option<usize>,
) -> Vec<char> {
    row.iter()
        .enumerate()
        .map(|(i, cell)| {
            if cursor_col == Some(i) && (cell.c == ' ' || cell.c == '\0') {
                '\u{2588}' // U+2588 FULL BLOCK
            } else {
                cell.c
            }
        })
        .collect()
}

/// Populate a single row `Buffer` with per-cell colored spans.
///
/// `cursor_col` — `Some(col)` when this row is the cursor row.  When the
/// cursor sits on a blank/space cell, U+2588 (FULL BLOCK) is substituted so
/// the text layer shows a visible glyph at the cursor position.  The
/// substitution is render-only; the stored [`SnapshotCell`] data is
/// never modified.
fn shape_row_into_buffer(
    buf: &mut Buffer,
    row: &[SnapshotCell],
    font_system: &mut FontSystem,
    palette: &RenderPalette,
    cursor_col: Option<usize>,
) {
    let chars = substitute_cursor_char(row, cursor_col);
    let span_strings: Vec<(String, Color, bool, bool)> = row
        .iter()
        .zip(chars.iter())
        .map(|(cell, &ch): (&SnapshotCell, &char)| {
            let s = ch.to_string();
            let fg = if cell.inverse {
                ansi_color_to_glyphon(cell.bg, false, palette)
            } else {
                ansi_color_to_glyphon(cell.fg, true, palette)
            };
            (s, fg, cell.bold, cell.italic)
        })
        .collect();

    buf.set_rich_text(
        font_system,
        span_strings.iter().map(|item: &(String, Color, bool, bool)| {
            let mut attrs = Attrs::new().family(Family::Monospace).color(item.1);
            if item.2 {
                attrs = attrs.weight(Weight::BOLD);
            }
            if item.3 {
                attrs = attrs.style(Style::Italic);
            }
            (item.0.as_str(), attrs)
        }),
        &Attrs::new().family(Family::Monospace),
        Shaping::Basic,
        None,
    );
    buf.shape_until_scroll(font_system, false);
}

/// Measure the pixel width/height of a single monospace cell using 'M'.
fn measure_cell(font_system: &mut FontSystem, font_size: f32, line_height: f32) -> CellSize {
    let mut buf = Buffer::new(font_system, Metrics::new(font_size, line_height));
    buf.set_size(font_system, Some(1000.0), Some(line_height * 2.0));
    buf.set_text(
        font_system,
        "M",
        &Attrs::new().family(Family::Monospace),
        Shaping::Basic,
        None,
    );
    buf.shape_until_scroll(font_system, false);

    // Walk layout runs to find the advance width of the 'M' glyph.
    let mut cell_width = font_size * 0.6; // fallback: 60% of font size
    if let Some(glyph) = buf.layout_runs().next().and_then(|run| run.glyphs.first()) {
        cell_width = glyph.w;
    }

    CellSize {
        width: cell_width,
        height: line_height,
    }
}

/// Map a `SnapshotColor` to a glyphon `Color` using the active palette.
///
/// `is_fg` controls the default: palette foreground for fg, palette
/// background for bg.
pub fn ansi_color_to_glyphon(color: SnapshotColor, is_fg: bool, palette: &RenderPalette) -> Color {
    match color {
        SnapshotColor::Default => {
            if is_fg {
                palette.fg_glyphon()
            } else {
                let (r, g, b) = palette.background;
                Color::rgb(r, g, b)
            }
        }
        SnapshotColor::Rgb(r, g, b) => Color::rgb(r, g, b),
        SnapshotColor::Indexed(n) => palette.indexed_glyphon(n),
    }
}

/// Compute a cheap hash of a row's visual content for dirty-row skipping.
///
/// The hash encodes:
/// - Each cell's character code point.
/// - Each cell's fg and bg color discriminants and values.
/// - Each cell's attribute flags (bold, italic, underline, inverse).
/// - The cursor column (`cursor_col`) when provided (so cursor movement
///   always invalidates the row hash).
pub fn hash_row(
    row: &[SnapshotCell],
    row_idx: usize,
    cursor_col: Option<usize>,
) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut h = DefaultHasher::new();
    row_idx.hash(&mut h);
    // Include cursor column so that cursor movement within a row is detected.
    if let Some(col) = cursor_col {
        col.hash(&mut h);
    }
    for cell in row {
        (cell.c as u32).hash(&mut h);
        hash_snapshot_color(cell.fg, &mut h);
        hash_snapshot_color(cell.bg, &mut h);
        cell.bold.hash(&mut h);
        cell.italic.hash(&mut h);
        cell.underline.hash(&mut h);
        cell.inverse.hash(&mut h);
    }
    h.finish()
}

fn hash_snapshot_color(c: SnapshotColor, h: &mut impl std::hash::Hasher) {
    use std::hash::Hash;
    match c {
        SnapshotColor::Default => 0u8.hash(h),
        SnapshotColor::Indexed(n) => {
            1u8.hash(h);
            n.hash(h);
        }
        SnapshotColor::Rgb(r, g, b) => {
            2u8.hash(h);
            r.hash(h);
            g.hash(h);
            b.hash(h);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::snapshot::{SnapshotCell, SnapshotColor};
    use super::{hash_row, substitute_cursor_char};

    fn default_row(n: usize) -> Vec<SnapshotCell> {
        (0..n).map(|_| SnapshotCell::default()).collect()
    }

    /// Two identical rows must produce the same hash.
    #[test]
    fn hash_row_identical_rows_match() {
        let row = default_row(5);
        let h1 = hash_row(&row, 0, None); // cursor not on this row
        let h2 = hash_row(&row, 0, None);
        assert_eq!(h1, h2, "identical rows must hash identically");
    }

    /// Changing a cell character must change the hash.
    #[test]
    fn hash_row_char_change_invalidates() {
        let mut row = default_row(5);
        let h1 = hash_row(&row, 0, None);
        row[2].c = 'X';
        let h2 = hash_row(&row, 0, None);
        assert_ne!(h1, h2, "character change must alter hash");
    }

    /// Moving the cursor within the cursor row must change the hash.
    #[test]
    fn hash_row_cursor_column_movement_invalidates() {
        let row = default_row(5);
        let h1 = hash_row(&row, 0, Some(1));
        let h2 = hash_row(&row, 0, Some(3));
        assert_ne!(h1, h2, "cursor column change in cursor row must alter hash");
    }

    /// Cursor movement in a non-cursor row must NOT change that row's hash.
    #[test]
    fn hash_row_cursor_movement_other_row_unchanged() {
        let row = default_row(5);
        // row_idx=0 with cursor_col=None (cursor is on a different row)
        let h1 = hash_row(&row, 0, None);
        let h2 = hash_row(&row, 0, None);
        assert_eq!(
            h1, h2,
            "cursor movement on another row must not change hash of row 0"
        );
    }

    /// Changing a cell's fg color must change the hash.
    #[test]
    fn hash_row_fg_color_change_invalidates() {
        let mut row = default_row(3);
        let h1 = hash_row(&row, 0, None);
        row[1].fg = SnapshotColor::Rgb(255, 0, 0);
        let h2 = hash_row(&row, 0, None);
        assert_ne!(h1, h2, "fg color change must alter hash");
    }

    /// Changing a cell's inverse flag must change the hash.
    #[test]
    fn hash_row_reverse_flag_invalidates() {
        let mut row = default_row(3);
        let h1 = hash_row(&row, 0, None);
        row[0].inverse = true;
        let h2 = hash_row(&row, 0, None);
        assert_ne!(h1, h2, "inverse attribute change must alter hash");
    }

    // ---- ISSUE-006 regression: cursor on blank cell uses block glyph ----

    /// When the cursor is on a blank (space) cell, the rendered character must
    /// be U+2588 (FULL BLOCK). All other cells must remain unchanged.
    #[test]
    fn cursor_on_blank_substitutes_block_glyph() {
        let row = default_row(5);
        // cursor_col=Some(2) means the cursor sits on column 2 of this row.
        let chars = substitute_cursor_char(&row, Some(2));
        assert_eq!(chars[2], '\u{2588}', "cursor on blank cell must yield U+2588");
        assert_eq!(chars[0], ' ', "non-cursor blank cells must remain space");
        assert_eq!(chars[1], ' ', "non-cursor blank cells must remain space");
        assert_eq!(chars[3], ' ', "non-cursor blank cells must remain space");
        assert_eq!(chars[4], ' ', "non-cursor blank cells must remain space");
    }

    /// When cursor_col is None (not the cursor row), no substitution is made.
    #[test]
    fn no_cursor_no_substitution() {
        let row = default_row(5);
        let chars = substitute_cursor_char(&row, None);
        assert!(
            chars.iter().all(|&c| c == ' '),
            "without cursor, all cells must remain space"
        );
    }

    /// A non-blank character at the cursor position must NOT be substituted.
    #[test]
    fn cursor_on_non_blank_no_substitution() {
        let mut row = default_row(5);
        row[2].c = 'A';
        let chars = substitute_cursor_char(&row, Some(2));
        assert_eq!(chars[2], 'A', "cursor on non-blank cell must not substitute");
    }
}
