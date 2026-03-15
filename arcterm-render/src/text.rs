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
        }
    }

    /// Update the viewport resolution (call before prepare each frame).
    pub fn update_viewport(&mut self, queue: &wgpu::Queue, width: u32, height: u32) {
        self.viewport.update(queue, Resolution { width, height });
    }

    /// Convert a Grid into glyphon TextAreas and upload to the atlas.
    pub fn prepare_grid(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        grid: &Grid,
        scale_factor: f32,
    ) -> Result<(), glyphon::PrepareError> {
        let rows = grid.rows();
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

        // Populate each row buffer with per-cell colored spans.
        for (row_idx, row) in rows.iter().enumerate() {
            let buf = &mut self.row_buffers[row_idx];
            let width_px = cell_w * row.len() as f32;
            buf.set_size(
                &mut self.font_system,
                Some(width_px * scale_factor),
                Some(cell_h * scale_factor),
            );

            // Build per-cell (text_slice, Attrs) spans.
            // We collect each char as a heap-allocated String so lifetimes work out.
            let span_strings: Vec<(String, Color)> = row
                .iter()
                .map(|cell| {
                    let s = cell.c.to_string();
                    let fg = ansi_color_to_glyphon(cell.attrs.fg, true);
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

    /// Render previously prepared text into the active render pass.
    pub fn render<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
    ) -> Result<(), glyphon::RenderError> {
        self.renderer.render(&self.atlas, &self.viewport, pass)
    }

    /// Trim the glyph atlas (call after frame submission).
    pub fn trim_atlas(&mut self) {
        self.atlas.trim();
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
    'outer: for run in buf.layout_runs() {
        for glyph in run.glyphs {
            cell_width = glyph.w;
            break 'outer;
        }
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
    if n >= 16 && n <= 231 {
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
