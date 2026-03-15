//! Text rendering using glyphon (cosmic-text + wgpu atlas).

use arcterm_core::{Color as TermColor, Grid};
use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer as GlyphonTextRenderer, Viewport,
};
use wgpu::MultisampleState;

/// Cell dimensions measured from the font metrics.
#[derive(Clone, Copy, Debug)]
pub struct CellSize {
    pub width: f32,
    pub height: f32,
}

/// Manages all glyphon state and converts a terminal Grid to GPU text draws.
pub struct TextRenderer {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub viewport: Viewport,
    pub atlas: TextAtlas,
    pub renderer: GlyphonTextRenderer,
    /// Per-row Buffers; resized as needed.
    row_buffers: Vec<Buffer>,
    pub cell_size: CellSize,
    /// Font size in logical pixels.
    font_size: f32,
    line_height: f32,
    /// Per-row content hashes for dirty-row optimization (Task 3).
    pub row_hashes: Vec<u64>,
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
        }
    }

    /// Update the viewport resolution (call before prepare each frame).
    pub fn update_viewport(&mut self, queue: &wgpu::Queue, width: u32, height: u32) {
        self.viewport.update(queue, Resolution { width, height });
    }

    /// Convert a Grid into glyphon TextAreas and upload to the atlas.
    ///
    /// Uses `rows_for_viewport()` so scrollback is rendered correctly.
    /// The cursor row is always re-shaped; other rows are skipped when their
    /// content hash is unchanged (dirty-row optimization, Task 3).
    pub fn prepare_grid(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        grid: &Grid,
        scale_factor: f32,
    ) -> Result<(), glyphon::PrepareError> {
        let rows = grid.rows_for_viewport();
        let num_rows = rows.len();
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

        let cursor = grid.cursor;

        // Populate each row buffer with per-cell colored spans.
        for (row_idx, row) in rows.iter().enumerate() {
            let is_cursor_row = row_idx == cursor.row;

            // Compute a cheap content hash for dirty-row skipping (Task 3).
            let row_hash = hash_row(row, row_idx, cursor);

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

            // Build per-cell (text_slice, Attrs) spans.
            // The quad pipeline handles background colors and the cursor block,
            // so here we only need to output the correct foreground color.
            // For cells with the `reverse` attribute the fg/bg are swapped:
            // the quad renderer draws the (original) fg as background, so the
            // text must be drawn in the (original) bg color.
            let span_strings: Vec<(String, Color)> = row
                .iter()
                .map(|cell| {
                    let s = cell.c.to_string();
                    let fg = if cell.attrs.reverse {
                        // Reverse: text draws with the cell's background color.
                        ansi_color_to_glyphon(cell.attrs.bg, false)
                    } else {
                        ansi_color_to_glyphon(cell.attrs.fg, true)
                    };
                    (s, fg)
                })
                .collect();

            // set_rich_text takes Iterator<Item = (&str, Attrs)>.
            buf.set_rich_text(
                &mut self.font_system,
                span_strings.iter().map(|(s, fg)| {
                    (s.as_str(), Attrs::new().family(Family::Monospace).color(*fg))
                }),
                &Attrs::new().family(Family::Monospace),
                Shaping::Basic,
                None,
            );
            buf.shape_until_scroll(&mut self.font_system, false);
        }

        // Build TextArea slice from the now-stable row_buffers.
        let text_areas: Vec<TextArea> = self
            .row_buffers
            .iter()
            .enumerate()
            .map(|(row_idx, buf)| TextArea {
                buffer: buf,
                left: 0.0,
                top: row_idx as f32 * cell_h,
                scale: scale_factor,
                bounds: TextBounds::default(),
                default_color: Color::rgb(0xd0, 0xd0, 0xd0),
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

/// Map a terminal Color to a glyphon Color.
/// `is_fg` controls the default: near-white for foreground, dark for background.
pub fn ansi_color_to_glyphon(color: TermColor, is_fg: bool) -> Color {
    match color {
        TermColor::Default => {
            if is_fg {
                Color::rgb(0xd0, 0xd0, 0xd0)
            } else {
                Color::rgb(0x1e, 0x1e, 0x2e)
            }
        }
        TermColor::Rgb(r, g, b) => Color::rgb(r, g, b),
        TermColor::Indexed(n) => indexed_to_glyphon(n),
    }
}

/// Convert an xterm-256 palette index to a glyphon Color.
fn indexed_to_glyphon(n: u8) -> Color {
    let (r, g, b) = indexed_to_rgb(n);
    Color::rgb(r, g, b)
}

fn indexed_to_rgb(n: u8) -> (u8, u8, u8) {
    // First 16: standard ANSI colors.
    static ANSI16: [(u8, u8, u8); 16] = [
        (0x00, 0x00, 0x00), // 0  black
        (0xcc, 0x00, 0x00), // 1  red
        (0x4e, 0x9a, 0x06), // 2  green
        (0xc4, 0xa0, 0x00), // 3  yellow
        (0x34, 0x65, 0xa4), // 4  blue
        (0x75, 0x50, 0x7b), // 5  magenta
        (0x06, 0x98, 0x9a), // 6  cyan
        (0xd3, 0xd7, 0xcf), // 7  white
        (0x55, 0x57, 0x53), // 8  bright black
        (0xef, 0x29, 0x29), // 9  bright red
        (0x8a, 0xe2, 0x34), // 10 bright green
        (0xfc, 0xe9, 0x4f), // 11 bright yellow
        (0x72, 0x9f, 0xcf), // 12 bright blue
        (0xad, 0x7f, 0xa8), // 13 bright magenta
        (0x34, 0xe2, 0xe2), // 14 bright cyan
        (0xee, 0xee, 0xec), // 15 bright white
    ];

    if (n as usize) < ANSI16.len() {
        return ANSI16[n as usize];
    }

    // 216-color cube: indices 16–231.
    if (16..=231).contains(&n) {
        let idx = n - 16;
        let b_idx = idx % 6;
        let g_idx = (idx / 6) % 6;
        let r_idx = idx / 36;
        let cube = |i: u8| if i == 0 { 0u8 } else { 55 + i * 40 };
        return (cube(r_idx), cube(g_idx), cube(b_idx));
    }

    // Greyscale ramp: indices 232–255.
    let level = (n - 232) * 10 + 8;
    (level, level, level)
}

/// Compute a cheap hash of a row's visual content for dirty-row skipping.
///
/// The hash encodes:
/// - Each cell's character code point.
/// - Each cell's fg and bg color discriminants and values.
/// - Each cell's attribute flags (bold, italic, underline, reverse).
/// - The cursor column when `row_idx` is the cursor row (so cursor movement
///   always invalidates the row hash).
pub fn hash_row(
    row: &[arcterm_core::Cell],
    row_idx: usize,
    cursor: arcterm_core::CursorPos,
) -> u64 {
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;

    let mut h = DefaultHasher::new();
    row_idx.hash(&mut h);
    // Include cursor column so that cursor movement within a row is detected.
    if row_idx == cursor.row {
        cursor.col.hash(&mut h);
    }
    for cell in row {
        (cell.c as u32).hash(&mut h);
        hash_color(cell.attrs.fg, &mut h);
        hash_color(cell.attrs.bg, &mut h);
        cell.attrs.bold.hash(&mut h);
        cell.attrs.italic.hash(&mut h);
        cell.attrs.underline.hash(&mut h);
        cell.attrs.reverse.hash(&mut h);
    }
    h.finish()
}

fn hash_color(c: TermColor, h: &mut impl std::hash::Hasher) {
    use std::hash::Hash;
    match c {
        TermColor::Default      => 0u8.hash(h),
        TermColor::Indexed(n)   => { 1u8.hash(h); n.hash(h); }
        TermColor::Rgb(r, g, b) => { 2u8.hash(h); r.hash(h); g.hash(h); b.hash(h); }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use arcterm_core::{Cell, Color as TermColor, CursorPos};
    use super::hash_row;

    /// Two identical rows must produce the same hash.
    #[test]
    fn hash_row_identical_rows_match() {
        let row: Vec<Cell> = (0..5).map(|_| Cell::default()).collect();
        let cursor = CursorPos { row: 1, col: 0 }; // not this row
        let h1 = hash_row(&row, 0, cursor);
        let h2 = hash_row(&row, 0, cursor);
        assert_eq!(h1, h2, "identical rows must hash identically");
    }

    /// Changing a cell character must change the hash.
    #[test]
    fn hash_row_char_change_invalidates() {
        let mut row: Vec<Cell> = (0..5).map(|_| Cell::default()).collect();
        let cursor = CursorPos { row: 1, col: 0 };
        let h1 = hash_row(&row, 0, cursor);
        row[2].c = 'X';
        let h2 = hash_row(&row, 0, cursor);
        assert_ne!(h1, h2, "character change must alter hash");
    }

    /// Moving the cursor within the cursor row must change the hash.
    #[test]
    fn hash_row_cursor_column_movement_invalidates() {
        let row: Vec<Cell> = (0..5).map(|_| Cell::default()).collect();
        let c1 = CursorPos { row: 0, col: 1 };
        let c2 = CursorPos { row: 0, col: 3 };
        let h1 = hash_row(&row, 0, c1);
        let h2 = hash_row(&row, 0, c2);
        assert_ne!(h1, h2, "cursor column change in cursor row must alter hash");
    }

    /// Cursor movement in a non-cursor row must NOT change that row's hash.
    #[test]
    fn hash_row_cursor_movement_other_row_unchanged() {
        let row: Vec<Cell> = (0..5).map(|_| Cell::default()).collect();
        let c1 = CursorPos { row: 3, col: 1 }; // cursor on different row
        let c2 = CursorPos { row: 3, col: 4 };
        let h1 = hash_row(&row, 0, c1);
        let h2 = hash_row(&row, 0, c2);
        assert_eq!(h1, h2, "cursor movement on another row must not change hash of row 0");
    }

    /// Changing a cell's fg color must change the hash.
    #[test]
    fn hash_row_fg_color_change_invalidates() {
        let mut row: Vec<Cell> = (0..3).map(|_| Cell::default()).collect();
        let cursor = CursorPos { row: 1, col: 0 };
        let h1 = hash_row(&row, 0, cursor);
        row[1].attrs.fg = TermColor::Rgb(255, 0, 0);
        let h2 = hash_row(&row, 0, cursor);
        assert_ne!(h1, h2, "fg color change must alter hash");
    }

    /// Changing a cell's reverse flag must change the hash.
    #[test]
    fn hash_row_reverse_flag_invalidates() {
        let mut row: Vec<Cell> = (0..3).map(|_| Cell::default()).collect();
        let cursor = CursorPos { row: 1, col: 0 };
        let h1 = hash_row(&row, 0, cursor);
        row[0].attrs.reverse = true;
        let h2 = hash_row(&row, 0, cursor);
        assert_ne!(h1, h2, "reverse attribute change must alter hash");
    }
}
