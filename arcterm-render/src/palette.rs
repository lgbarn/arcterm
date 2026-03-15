//! Render-side colour palette used by the wgpu renderer.
//!
//! [`RenderPalette`] carries only the data the renderer needs at draw time:
//! foreground, background, cursor, and the 16 ANSI terminal colours.  It is
//! kept separate from the app-level `ColorPalette` (which lives in
//! `arcterm-app`) so the render crate does not need to depend on the app crate.

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
    /// Catppuccin Mocha defaults — matches the app-level default palette.
    pub fn default_mocha() -> Self {
        Self {
            foreground: (0xcd, 0xd6, 0xf4),
            background: (0x1e, 0x1e, 0x2e),
            cursor:     (0xf5, 0xe0, 0xdc),
            ansi: [
                (0x45, 0x47, 0x5a),
                (0xf3, 0x8b, 0xa8),
                (0xa6, 0xe3, 0xa1),
                (0xf9, 0xe2, 0xaf),
                (0x89, 0xb4, 0xfa),
                (0xcb, 0xa6, 0xf7),
                (0x89, 0xdc, 0xeb),
                (0xba, 0xc2, 0xde),
                (0x58, 0x5b, 0x70),
                (0xf3, 0x8b, 0xa8),
                (0xa6, 0xe3, 0xa1),
                (0xf9, 0xe2, 0xaf),
                (0x89, 0xb4, 0xfa),
                (0xcb, 0xa6, 0xf7),
                (0x89, 0xdc, 0xeb),
                (0xcd, 0xd6, 0xf4),
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
    /// table for indices 0–15.
    pub fn indexed_rgb(&self, n: u8) -> (u8, u8, u8) {
        if (n as usize) < self.ansi.len() {
            return self.ansi[n as usize];
        }
        // 216-colour cube: 16–231.
        if (16..=231).contains(&n) {
            let idx = n - 16;
            let b_idx = idx % 6;
            let g_idx = (idx / 6) % 6;
            let r_idx = idx / 36;
            let cube = |i: u8| if i == 0 { 0u8 } else { 55 + i * 40 };
            return (cube(r_idx), cube(g_idx), cube(b_idx));
        }
        // Greyscale ramp: 232–255.
        let level = (n - 232) * 10 + 8;
        (level, level, level)
    }
}

impl Default for RenderPalette {
    fn default() -> Self {
        Self::default_mocha()
    }
}
