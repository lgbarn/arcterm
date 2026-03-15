//! Text selection model, pixel→cell mapping, word boundaries, and clipboard.

use arcterm_core::{Cell, Grid};

// ---------------------------------------------------------------------------
// SelectionQuad — a pixel-space rectangle representing one selected cell row
// ---------------------------------------------------------------------------

/// A rectangle in physical pixel coordinates that should be drawn as a
/// selection highlight.  One quad is emitted per contiguous selected row span.
///
/// x, y, width, height are all in physical pixels (after HiDPI scaling).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionQuad {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Generate the list of `SelectionQuad`s for the current selection.
///
/// # Parameters
/// - `selection`: the active selection (may be `SelectionMode::None`).
/// - `rows`: total visible rows in the viewport.
/// - `cols`: total columns in the grid.
/// - `cell_w`: logical cell width in points.
/// - `cell_h`: logical cell height in points.
/// - `scale`: HiDPI scale factor (physical pixels per logical point).
///
/// Returns an empty `Vec` if there is no active selection.
pub fn generate_selection_quads(
    selection: &Selection,
    rows: usize,
    cols: usize,
    cell_w: f32,
    cell_h: f32,
    scale: f32,
) -> Vec<SelectionQuad> {
    if selection.mode == SelectionMode::None {
        return Vec::new();
    }

    let (start, end) = selection.normalized();

    // Clamp to visible viewport.
    let row_start = start.row.min(rows.saturating_sub(1));
    let row_end = end.row.min(rows.saturating_sub(1));

    let mut quads = Vec::new();

    for r in row_start..=row_end {
        let col_start = if r == start.row { start.col } else { 0 };
        let col_end = if r == end.row { end.col + 1 } else { cols };
        let col_end = col_end.min(cols);

        if col_start >= col_end {
            continue;
        }

        let x = col_start as f32 * cell_w * scale;
        let y = r as f32 * cell_h * scale;
        let width = (col_end - col_start) as f32 * cell_w * scale;
        let height = cell_h * scale;

        quads.push(SelectionQuad { x, y, width, height });
    }

    quads
}

// ---------------------------------------------------------------------------
// CellPos — a position in the terminal grid
// ---------------------------------------------------------------------------

/// A (row, col) position in the terminal grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CellPos {
    pub row: usize,
    pub col: usize,
}

// ---------------------------------------------------------------------------
// SelectionMode
// ---------------------------------------------------------------------------

/// Granularity of the active selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SelectionMode {
    #[default]
    None,
    /// Single-click drag — individual characters.
    Character,
    /// Double-click — select by word boundaries.
    Word,
    /// Triple-click — select whole lines.
    Line,
}

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

/// An active text selection anchored at one cell and ending at another.
#[derive(Clone, Debug, Default)]
pub struct Selection {
    pub anchor: CellPos,
    pub end: CellPos,
    pub mode: SelectionMode,
}

impl Selection {
    /// Start a new selection at `pos` with the given `mode`.
    pub fn start(&mut self, pos: CellPos, mode: SelectionMode) {
        self.anchor = pos;
        self.end = pos;
        self.mode = mode;
    }

    /// Extend the selection to `pos` (mouse-move or shift-click).
    pub fn update(&mut self, pos: CellPos) {
        self.end = pos;
    }

    /// Return the selection normalized so that `start <= end` in reading order.
    ///
    /// Reading order: row-major, then col. The returned tuple is
    /// `(top_left, bottom_right)`.
    pub fn normalized(&self) -> (CellPos, CellPos) {
        let a = self.anchor;
        let e = self.end;
        if (a.row, a.col) <= (e.row, e.col) {
            (a, e)
        } else {
            (e, a)
        }
    }

    /// Return `true` if `pos` is within the (normalized) selection.
    #[allow(dead_code)] // Used by tests; production use in Wave 3 (selection quad rendering)
    pub fn contains(&self, pos: CellPos) -> bool {
        if self.mode == SelectionMode::None {
            return false;
        }
        let (start, end) = self.normalized();
        (pos.row, pos.col) >= (start.row, start.col)
            && (pos.row, pos.col) <= (end.row, end.col)
    }

    /// Extract the selected text from `grid`.
    ///
    /// Rows are separated by `\n`. Trailing spaces on each line are preserved
    /// unless the selection ends before the last non-space character.
    pub fn extract_text(&self, grid: &Grid) -> String {
        if self.mode == SelectionMode::None {
            return String::new();
        }
        let (start, end) = self.normalized();
        let mut result = String::new();

        for row_idx in start.row..=end.row {
            if row_idx > 0 && row_idx > start.row {
                result.push('\n');
            }
            let col_start = if row_idx == start.row { start.col } else { 0 };
            let col_end_exclusive = if row_idx == end.row {
                end.col + 1
            } else {
                grid.size.cols
            };
            if let Some(row) = grid.cells.get(row_idx) {
                let col_end_exclusive = col_end_exclusive.min(row.len());
                for cell in &row[col_start..col_end_exclusive] {
                    result.push(cell.c);
                }
            }
        }
        result
    }

    /// Clear the selection (set mode to None).
    pub fn clear(&mut self) {
        self.mode = SelectionMode::None;
        self.anchor = CellPos::default();
        self.end = CellPos::default();
    }
}

// ---------------------------------------------------------------------------
// Pixel → Cell conversion
// ---------------------------------------------------------------------------

/// Convert a physical pixel position (x, y) to a grid cell position.
///
/// - `cell_w`, `cell_h` are the logical cell dimensions in points.
/// - `scale` is the HiDPI scale factor (physical pixels per logical point).
///
/// Physical pixels are divided by scale to get logical coordinates, then
/// divided by cell dimensions to get the grid column/row.
pub fn pixel_to_cell(
    x: f64,
    y: f64,
    cell_w: f64,
    cell_h: f64,
    scale: f64,
) -> CellPos {
    let logical_x = x / scale;
    let logical_y = y / scale;
    let col = (logical_x / cell_w).floor() as usize;
    let row = (logical_y / cell_h).floor() as usize;
    CellPos { row, col }
}

// ---------------------------------------------------------------------------
// Word boundaries
// ---------------------------------------------------------------------------

/// Determine the start and end column indices (inclusive) of the word that
/// contains `col` in `row`.
///
/// A "word" is defined as a contiguous run of non-whitespace characters.
/// If `col` is on a whitespace character the boundary collapses to `(col, col)`.
#[allow(dead_code)] // Used by tests; production use in Wave 3
pub fn word_boundaries(row: &[Cell], col: usize) -> (usize, usize) {
    if col >= row.len() {
        return (col, col);
    }
    // If the character under col is whitespace, return degenerate boundary.
    if row[col].c.is_whitespace() {
        return (col, col);
    }
    // Scan left to find start.
    let mut start = col;
    while start > 0 && !row[start - 1].c.is_whitespace() {
        start -= 1;
    }
    // Scan right to find end.
    let mut end = col;
    while end + 1 < row.len() && !row[end + 1].c.is_whitespace() {
        end += 1;
    }
    (start, end)
}

// ---------------------------------------------------------------------------
// Clipboard
// ---------------------------------------------------------------------------

/// A thin wrapper around `arboard::Clipboard` with owned text copy/paste.
pub struct Clipboard {
    inner: arboard::Clipboard,
}

impl Clipboard {
    /// Create a new clipboard context.
    ///
    /// # Errors
    ///
    /// Returns an error string if the system clipboard cannot be initialised
    /// (e.g. no display server on headless systems).
    pub fn new() -> Result<Self, String> {
        arboard::Clipboard::new()
            .map(|inner| Clipboard { inner })
            .map_err(|e| e.to_string())
    }

    /// Write `text` to the system clipboard.
    pub fn copy(&mut self, text: &str) -> Result<(), String> {
        self.inner
            .set_text(text.to_owned())
            .map_err(|e| e.to_string())
    }

    /// Read a string from the system clipboard.
    pub fn paste(&mut self) -> Result<String, String> {
        self.inner.get_text().map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Unit tests (TDD — written before implementation)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use arcterm_core::{Grid, GridSize};

    // -----------------------------------------------------------------------
    // Selection::normalized — order
    // -----------------------------------------------------------------------

    #[test]
    fn normalized_order_forward() {
        let mut sel = Selection::default();
        sel.start(CellPos { row: 1, col: 3 }, SelectionMode::Character);
        sel.update(CellPos { row: 2, col: 5 });
        let (s, e) = sel.normalized();
        assert_eq!(s, CellPos { row: 1, col: 3 });
        assert_eq!(e, CellPos { row: 2, col: 5 });
    }

    #[test]
    fn normalized_order_backward() {
        let mut sel = Selection::default();
        sel.start(CellPos { row: 3, col: 7 }, SelectionMode::Character);
        sel.update(CellPos { row: 1, col: 2 });
        let (s, e) = sel.normalized();
        assert_eq!(s, CellPos { row: 1, col: 2 });
        assert_eq!(e, CellPos { row: 3, col: 7 });
    }

    #[test]
    fn normalized_same_row_backward_col() {
        let mut sel = Selection::default();
        sel.start(CellPos { row: 2, col: 8 }, SelectionMode::Character);
        sel.update(CellPos { row: 2, col: 3 });
        let (s, e) = sel.normalized();
        assert_eq!(s.col, 3);
        assert_eq!(e.col, 8);
    }

    // -----------------------------------------------------------------------
    // Selection::contains — multi-row
    // -----------------------------------------------------------------------

    #[test]
    fn contains_middle_row_is_included() {
        let mut sel = Selection::default();
        sel.start(CellPos { row: 1, col: 0 }, SelectionMode::Character);
        sel.update(CellPos { row: 3, col: 5 });
        // Middle row, any column within bounds, should be contained.
        assert!(sel.contains(CellPos { row: 2, col: 0 }));
        assert!(sel.contains(CellPos { row: 2, col: 79 }));
    }

    #[test]
    fn contains_respects_start_col_on_first_row() {
        let mut sel = Selection::default();
        sel.start(CellPos { row: 1, col: 4 }, SelectionMode::Character);
        sel.update(CellPos { row: 3, col: 5 });
        // col 3 on row 1 is before start.col — NOT contained.
        assert!(!sel.contains(CellPos { row: 1, col: 3 }));
        // col 4 on row 1 is start.col — contained.
        assert!(sel.contains(CellPos { row: 1, col: 4 }));
    }

    #[test]
    fn contains_respects_end_col_on_last_row() {
        let mut sel = Selection::default();
        sel.start(CellPos { row: 1, col: 0 }, SelectionMode::Character);
        sel.update(CellPos { row: 3, col: 5 });
        // col 5 on row 3 is end.col — contained.
        assert!(sel.contains(CellPos { row: 3, col: 5 }));
        // col 6 on row 3 is past end.col — NOT contained.
        assert!(!sel.contains(CellPos { row: 3, col: 6 }));
    }

    #[test]
    fn contains_none_mode_always_false() {
        let sel = Selection::default(); // mode == None
        assert!(!sel.contains(CellPos { row: 0, col: 0 }));
    }

    // -----------------------------------------------------------------------
    // Selection::extract_text
    // -----------------------------------------------------------------------

    fn make_grid_with_text(rows: &[&str]) -> Grid {
        let num_rows = rows.len();
        let num_cols = rows.iter().map(|r| r.len()).max().unwrap_or(1);
        let mut g = Grid::new(GridSize::new(num_rows, num_cols));
        for (r, text) in rows.iter().enumerate() {
            for (c, ch) in text.chars().enumerate() {
                g.cells[r][c].c = ch;
            }
        }
        g
    }

    #[test]
    fn extract_text_single_row() {
        let grid = make_grid_with_text(&["Hello World"]);
        let mut sel = Selection::default();
        sel.start(CellPos { row: 0, col: 0 }, SelectionMode::Character);
        sel.update(CellPos { row: 0, col: 4 }); // "Hello"
        let text = sel.extract_text(&grid);
        assert_eq!(text, "Hello");
    }

    #[test]
    fn extract_text_multi_row() {
        let grid = make_grid_with_text(&["ABCDE", "FGHIJ"]);
        let mut sel = Selection::default();
        sel.start(CellPos { row: 0, col: 2 }, SelectionMode::Character);
        sel.update(CellPos { row: 1, col: 2 }); // "CDE\nFGH"
        let text = sel.extract_text(&grid);
        assert_eq!(text, "CDE\nFGH");
    }

    #[test]
    fn extract_text_none_mode_returns_empty() {
        let grid = make_grid_with_text(&["Hello"]);
        let sel = Selection::default();
        assert_eq!(sel.extract_text(&grid), "");
    }

    // -----------------------------------------------------------------------
    // pixel_to_cell
    // -----------------------------------------------------------------------

    #[test]
    fn pixel_to_cell_origin() {
        let pos = pixel_to_cell(0.0, 0.0, 8.0, 16.0, 1.0);
        assert_eq!(pos, CellPos { row: 0, col: 0 });
    }

    #[test]
    fn pixel_to_cell_exact_boundary() {
        // Physical pixel (16, 32) with scale 1.0, cell 8×16 → col 2, row 2.
        let pos = pixel_to_cell(16.0, 32.0, 8.0, 16.0, 1.0);
        assert_eq!(pos, CellPos { row: 2, col: 2 });
    }

    #[test]
    fn pixel_to_cell_hidpi_scale() {
        // Physical pixel (32, 64) with scale 2.0 → logical (16, 32).
        // Cell 8×16: col = 16/8 = 2, row = 32/16 = 2.
        let pos = pixel_to_cell(32.0, 64.0, 8.0, 16.0, 2.0);
        assert_eq!(pos, CellPos { row: 2, col: 2 });
    }

    #[test]
    fn pixel_to_cell_partial_cell() {
        // x=10.0, cell_w=8.0 → col=floor(1.25)=1
        let pos = pixel_to_cell(10.0, 5.0, 8.0, 16.0, 1.0);
        assert_eq!(pos.col, 1);
        assert_eq!(pos.row, 0);
    }

    // -----------------------------------------------------------------------
    // word_boundaries
    // -----------------------------------------------------------------------

    fn make_row(s: &str) -> Vec<Cell> {
        s.chars()
            .map(|c| Cell { c, ..Cell::default() })
            .collect()
    }

    #[test]
    fn word_boundaries_single_word() {
        let row = make_row("hello world");
        let (start, end) = word_boundaries(&row, 2); // 'l' in "hello"
        assert_eq!(start, 0);
        assert_eq!(end, 4); // "hello" spans 0-4
    }

    #[test]
    fn word_boundaries_second_word() {
        let row = make_row("hello world");
        let (start, end) = word_boundaries(&row, 7); // 'o' in "world"
        assert_eq!(start, 6);
        assert_eq!(end, 10);
    }

    #[test]
    fn word_boundaries_on_whitespace() {
        let row = make_row("hello world");
        let (start, end) = word_boundaries(&row, 5); // space
        assert_eq!(start, 5);
        assert_eq!(end, 5);
    }

    #[test]
    fn word_boundaries_at_end_of_row() {
        let row = make_row("abc");
        let (start, end) = word_boundaries(&row, 2); // 'c'
        assert_eq!(start, 0);
        assert_eq!(end, 2);
    }

    #[test]
    fn word_boundaries_single_char_word() {
        let row = make_row("a b");
        let (start, end) = word_boundaries(&row, 0); // 'a'
        assert_eq!(start, 0);
        assert_eq!(end, 0);
    }

    // -----------------------------------------------------------------------
    // generate_selection_quads
    // -----------------------------------------------------------------------

    #[test]
    fn quads_empty_when_no_selection() {
        let sel = Selection::default(); // mode == None
        let quads = generate_selection_quads(&sel, 24, 80, 8.0, 16.0, 1.0);
        assert!(quads.is_empty());
    }

    #[test]
    fn quads_single_row_single_cell() {
        let mut sel = Selection::default();
        sel.start(CellPos { row: 0, col: 2 }, SelectionMode::Character);
        sel.update(CellPos { row: 0, col: 2 });
        let quads = generate_selection_quads(&sel, 24, 80, 8.0, 16.0, 1.0);
        assert_eq!(quads.len(), 1);
        let q = quads[0];
        assert!((q.x - 16.0).abs() < 1e-3, "x = col*cell_w = 2*8 = 16");
        assert!((q.y - 0.0).abs() < 1e-3, "y = row*cell_h = 0*16 = 0");
        assert!((q.width - 8.0).abs() < 1e-3, "width = 1 cell * 8");
        assert!((q.height - 16.0).abs() < 1e-3, "height = cell_h = 16");
    }

    #[test]
    fn quads_multi_row_produces_one_quad_per_row() {
        let mut sel = Selection::default();
        sel.start(CellPos { row: 1, col: 0 }, SelectionMode::Character);
        sel.update(CellPos { row: 3, col: 4 });
        let quads = generate_selection_quads(&sel, 24, 80, 8.0, 16.0, 1.0);
        // Rows 1, 2, 3 → 3 quads
        assert_eq!(quads.len(), 3);
    }

    #[test]
    fn quads_hidpi_scale() {
        let mut sel = Selection::default();
        sel.start(CellPos { row: 0, col: 0 }, SelectionMode::Character);
        sel.update(CellPos { row: 0, col: 1 }); // 2 cells
        // scale=2 → physical pixels doubled
        let quads = generate_selection_quads(&sel, 24, 80, 8.0, 16.0, 2.0);
        assert_eq!(quads.len(), 1);
        let q = quads[0];
        assert!((q.width - 32.0).abs() < 1e-3, "width = 2 cells * 8 * 2 = 32");
        assert!((q.height - 32.0).abs() < 1e-3, "height = 16 * 2 = 32");
    }

    #[test]
    fn quads_clamped_to_viewport_rows() {
        let mut sel = Selection::default();
        sel.start(CellPos { row: 0, col: 0 }, SelectionMode::Character);
        // end row beyond viewport
        sel.update(CellPos { row: 100, col: 5 });
        let quads = generate_selection_quads(&sel, 5, 10, 8.0, 16.0, 1.0);
        // Rows 0-4 only (viewport has 5 rows).
        assert_eq!(quads.len(), 5);
    }
}
