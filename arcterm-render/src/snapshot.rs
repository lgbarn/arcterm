//! RenderSnapshot — a lock-free copy of the terminal state needed for one frame.
//!
//! The renderer must not hold the alacritty `FairMutex` during GPU work; that
//! would block the reader thread from advancing the VT parser.  Instead the app
//! layer locks the `Term`, calls `snapshot_from_term`, and immediately releases
//! the lock.  The returned `RenderSnapshot` is then passed to the renderer.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::ansi::Color as VteColor;
use alacritty_terminal::vte::ansi::CursorShape;

// ---------------------------------------------------------------------------
// SnapshotColor
// ---------------------------------------------------------------------------

/// A terminal colour resolved from the alacritty VTE palette.
///
/// The three variants map directly to `vte::ansi::Color`:
/// - `Named` / bare indexed colours both become `Indexed`.
/// - True-colour RGB becomes `Rgb`.
/// - Unset fg / bg becomes `Default`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SnapshotColor {
    /// Use the palette default (fg or bg depending on context).
    Default,
    /// 256-colour palette index (0-255).  Named colours are mapped to their
    /// canonical index (`NamedColor as u8`) before storage.
    Indexed(u8),
    /// 24-bit true colour: (red, green, blue).
    Rgb(u8, u8, u8),
}

// ---------------------------------------------------------------------------
// SnapshotCell
// ---------------------------------------------------------------------------

/// A single terminal cell captured at snapshot time.
///
/// Attribute flags are decoded from `alacritty_terminal::term::cell::Flags`
/// into plain booleans so the renderer does not need to import the flags type.
#[derive(Clone, Debug)]
pub struct SnapshotCell {
    /// The Unicode scalar value to render.  Null / zero-width spacers use `' '`.
    pub c: char,
    /// Foreground colour.
    pub fg: SnapshotColor,
    /// Background colour.
    pub bg: SnapshotColor,
    /// SGR bold attribute.
    pub bold: bool,
    /// SGR italic attribute.
    pub italic: bool,
    /// SGR underline attribute (any underline style).
    pub underline: bool,
    /// SGR reverse-video attribute.
    pub inverse: bool,
}

impl Default for SnapshotCell {
    fn default() -> Self {
        Self {
            c: ' ',
            fg: SnapshotColor::Default,
            bg: SnapshotColor::Default,
            bold: false,
            italic: false,
            underline: false,
            inverse: false,
        }
    }
}

// ---------------------------------------------------------------------------
// RenderSnapshot
// ---------------------------------------------------------------------------

/// A complete, lock-free snapshot of the terminal state required for one frame.
///
/// Cells are stored in **row-major order**: cell at `(row, col)` is at index
/// `row * cols + col`.
pub struct RenderSnapshot {
    /// Flat cell buffer, row-major.  Length is always `rows * cols`.
    pub cells: Vec<SnapshotCell>,
    /// Number of columns.
    pub cols: usize,
    /// Number of visible rows.
    pub rows: usize,
    /// Cursor row (0-indexed from the top of the visible viewport).
    pub cursor_row: usize,
    /// Cursor column (0-indexed).
    pub cursor_col: usize,
    /// Whether the cursor should be drawn.
    ///
    /// This is `false` when the `SHOW_CURSOR` terminal mode is off *or* when
    /// the cursor shape is `CursorShape::Hidden`.
    pub cursor_visible: bool,
    /// Cursor shape as reported by the terminal.
    pub cursor_shape: CursorShape,
}

impl RenderSnapshot {
    /// Return the cell at `(row, col)`, or `None` if out of bounds.
    #[inline]
    pub fn cell(&self, row: usize, col: usize) -> Option<&SnapshotCell> {
        if row < self.rows && col < self.cols {
            Some(&self.cells[row * self.cols + col])
        } else {
            None
        }
    }

    /// Return the row slice at `row_idx`.
    #[inline]
    pub fn row(&self, row_idx: usize) -> &[SnapshotCell] {
        let start = row_idx * self.cols;
        &self.cells[start..start + self.cols]
    }
}

// ---------------------------------------------------------------------------
// snapshot_from_term
// ---------------------------------------------------------------------------

/// Extract a `RenderSnapshot` from an alacritty `Term`.
///
/// The caller must hold (or acquire) the `FairMutex` guard before calling
/// this function and release it immediately afterwards so the reader thread
/// is not blocked during GPU rendering.
///
/// # Mapping
///
/// - `display_iter` provides the visible viewport cells in row-major order.
///   Each `Indexed<&Cell>` carries `.point.line` (i32, 0 = top visible row)
///   and `.point.column` (usize).
/// - `content.cursor` provides the cursor position and shape.  When the
///   cursor shape is `CursorShape::Hidden`, `cursor_visible` is set to `false`.
pub fn snapshot_from_term<E: EventListener>(term: &Term<E>) -> RenderSnapshot {
    use alacritty_terminal::grid::Dimensions;
    use alacritty_terminal::term::cell::Flags;

    let cols = term.columns();
    let rows = term.screen_lines();
    let total = rows * cols;

    // Allocate the flat cell buffer filled with defaults.
    let mut cells: Vec<SnapshotCell> = Vec::with_capacity(total);
    cells.resize_with(total, SnapshotCell::default);

    let content = term.renderable_content();

    // Fill cells from the display iterator.
    for indexed in content.display_iter {
        let row_i = indexed.point.line.0; // i32
        let col_i = indexed.point.column.0; // usize

        // Skip cells outside the visible viewport (negative line means
        // scrollback, which we do not render here).
        if row_i < 0 || (row_i as usize) >= rows || col_i >= cols {
            continue;
        }

        let row = row_i as usize;
        let slot = &mut cells[row * cols + col_i];

        // Character — treat null as space.
        slot.c = if indexed.c == '\0' { ' ' } else { indexed.c };

        // Colours.
        slot.fg = vte_color_to_snapshot(indexed.fg);
        slot.bg = vte_color_to_snapshot(indexed.bg);

        // Attribute flags.
        slot.bold = indexed.flags.contains(Flags::BOLD);
        slot.italic = indexed.flags.contains(Flags::ITALIC);
        slot.underline = indexed.flags.intersects(Flags::ALL_UNDERLINES);
        slot.inverse = indexed.flags.contains(Flags::INVERSE);
    }

    // Cursor.
    let cursor = content.cursor;
    let cursor_shape = cursor.shape;
    let cursor_visible = cursor_shape != CursorShape::Hidden;
    let cur_row = cursor.point.line.0.max(0) as usize;
    let cur_col = cursor.point.column.0;

    RenderSnapshot {
        cells,
        cols,
        rows,
        cursor_row: cur_row.min(rows.saturating_sub(1)),
        cursor_col: cur_col.min(cols.saturating_sub(1)),
        cursor_visible,
        cursor_shape,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map a `vte::ansi::Color` to a `SnapshotColor`.
fn vte_color_to_snapshot(color: VteColor) -> SnapshotColor {
    match color {
        VteColor::Named(n) => SnapshotColor::Indexed(n as u8),
        VteColor::Indexed(i) => SnapshotColor::Indexed(i),
        VteColor::Spec(rgb) => SnapshotColor::Rgb(rgb.r, rgb.g, rgb.b),
    }
}
