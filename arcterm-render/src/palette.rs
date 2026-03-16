//! Render-side colour palette used by the wgpu renderer.
//!
//! [`RenderPalette`] carries only the data the renderer needs at draw time:
//! foreground, background, cursor, and the 16 ANSI terminal colours.  It is
//! kept separate from the app-level `ColorPalette` (which lives in
//! `arcterm-app`) so the render crate does not need to depend on the app crate.

// ---------------------------------------------------------------------------
// Pre-computed xterm-256 lookup table
// ---------------------------------------------------------------------------

/// O(1) lookup table for xterm-256 colour indices.
///
/// Indices 0–15 are the ANSI colours — stored as `(0, 0, 0)` here because
/// the actual values are palette-specific and looked up from `RenderPalette::ansi`.
/// Indices 16–231 map to the 216-colour cube; 232–255 to the greyscale ramp.
pub(crate) const XTERM_256: [(u8, u8, u8); 256] = {
    let mut table = [(0u8, 0u8, 0u8); 256];
    // Indices 0–15: left as (0, 0, 0); caller falls back to self.ansi[].
    // Indices 16–231: 6×6×6 colour cube.
    let mut i = 16usize;
    while i <= 231 {
        let idx = (i - 16) as u8;
        let b_idx = idx % 6;
        let g_idx = (idx / 6) % 6;
        let r_idx = idx / 36;
        let r = if r_idx == 0 { 0u8 } else { 55 + r_idx * 40 };
        let g = if g_idx == 0 { 0u8 } else { 55 + g_idx * 40 };
        let b = if b_idx == 0 { 0u8 } else { 55 + b_idx * 40 };
        table[i] = (r, g, b);
        i += 1;
    }
    // Indices 232–255: greyscale ramp.
    let mut i = 232usize;
    while i < 256 {
        let level = (i as u8 - 232) * 10 + 8;
        table[i] = (level, level, level);
        i += 1;
    }
    table
};

/// Complete colour palette for a single terminal frame.
///
/// All colours are `(r, g, b)` tuples of `u8` values (0–255).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderPalette {
    /// Default foreground colour.
    pub foreground: (u8, u8, u8),
    /// Default background colour.
    pub background: (u8, u8, u8),
    /// Cursor block colour.
    pub cursor: (u8, u8, u8),
    /// ANSI colours 0–15.
    pub ansi: [(u8, u8, u8); 16],
}

impl RenderPalette {
    /// Cool-Night defaults — matches the app-level default palette.
    pub fn default_cool_night() -> Self {
        Self {
            foreground: (0xcb, 0xe0, 0xf0),
            background: (0x01, 0x14, 0x23),
            cursor:     (0x47, 0xff, 0x9c),
            ansi: [
                (0x0b, 0x25, 0x3a),
                (0xe8, 0x52, 0x52),
                (0x44, 0xff, 0xb1),
                (0xff, 0xd2, 0x7a),
                (0x1f, 0xa8, 0xff),
                (0xa2, 0x77, 0xff),
                (0x24, 0xea, 0xf7),
                (0xbe, 0xe5, 0xff),
                (0x4b, 0x6a, 0x87),
                (0xff, 0x6b, 0x6b),
                (0x67, 0xff, 0xc3),
                (0xff, 0xe8, 0x9a),
                (0x4d, 0xb5, 0xff),
                (0xc0, 0x9b, 0xff),
                (0x5a, 0xed, 0xff),
                (0xff, 0xff, 0xff),
            ],
        }
    }

    /// Convert background to a `wgpu::Color` for use as the clear load op.
    pub fn bg_wgpu(&self) -> wgpu::Color {
        let (r, g, b) = self.background;
        wgpu::Color {
            r: r as f64 / 255.0,
            g: g as f64 / 255.0,
            b: b as f64 / 255.0,
            a: 1.0,
        }
    }

    /// Convert background to an RGBA f32 array.
    pub fn bg_f32(&self) -> [f32; 4] {
        let (r, g, b) = self.background;
        [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
    }

    /// Convert foreground to a `glyphon::Color`.
    pub fn fg_glyphon(&self) -> glyphon::Color {
        let (r, g, b) = self.foreground;
        glyphon::Color::rgb(r, g, b)
    }

    /// Convert cursor to an RGBA f32 array.
    pub fn cursor_f32(&self) -> [f32; 4] {
        let (r, g, b) = self.cursor;
        [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
    }

    /// Convert ANSI index `n` (0–15) to a `glyphon::Color`.
    ///
    /// For indices 0–15 the palette's own ANSI table is used.
    /// Indices 16–231 map to the 216-colour cube; 232–255 to the greyscale ramp.
    pub fn indexed_glyphon(&self, n: u8) -> glyphon::Color {
        let (r, g, b) = self.indexed_rgb(n);
        glyphon::Color::rgb(r, g, b)
    }

    /// Convert an xterm-256 index to `(r, g, b)`, using the palette's ANSI
    /// table for indices 0–15 and the pre-computed [`XTERM_256`] table for 16–255.
    pub fn indexed_rgb(&self, n: u8) -> (u8, u8, u8) {
        if (n as usize) < self.ansi.len() {
            return self.ansi[n as usize];
        }
        XTERM_256[n as usize]
    }
}

impl Default for RenderPalette {
    fn default() -> Self {
        Self::default_cool_night()
    }
}
