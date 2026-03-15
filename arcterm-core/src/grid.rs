//! Terminal grid and cursor types.

use crate::cell::{Cell, CellAttrs, Color};

/// Position of the cursor in the terminal grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CursorPos {
    pub row: usize,
    pub col: usize,
}

/// Dimensions of the terminal grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct GridSize {
    pub rows: usize,
    pub cols: usize,
}

impl GridSize {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self { rows, cols }
    }
}

/// The terminal cell grid.
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    pub cells: Vec<Vec<Cell>>,
    pub size: GridSize,
    pub cursor: CursorPos,
    pub dirty: bool,
    /// Current text attributes applied to new characters.
    pub current_attrs: CellAttrs,
    /// Window title (OSC 0/2).
    pub title: Option<String>,
}

impl Grid {
    /// Allocate a new grid with the given dimensions.
    pub fn new(size: GridSize) -> Self {
        let cells = (0..size.rows)
            .map(|_| (0..size.cols).map(|_| Cell::default()).collect())
            .collect();
        Self {
            cells,
            size,
            cursor: CursorPos::default(),
            dirty: true,
            current_attrs: CellAttrs::default(),
            title: None,
        }
    }

    /// Immutable cell access. Returns None on out-of-bounds.
    pub fn cell_opt(&self, row: usize, col: usize) -> Option<&Cell> {
        self.cells.get(row)?.get(col)
    }

    /// Immutable cell access (panics on out-of-bounds — kept for existing tests).
    pub fn cell(&self, row: usize, col: usize) -> &Cell {
        &self.cells[row][col]
    }

    /// Mutable cell access; marks the grid dirty.
    pub fn cell_mut(&mut self, row: usize, col: usize) -> &mut Cell {
        self.dirty = true;
        &mut self.cells[row][col]
    }

    /// Return the current cursor position.
    pub fn cursor(&self) -> CursorPos {
        self.cursor
    }

    /// Set the cursor position with bounds clamping.
    pub fn set_cursor(&mut self, pos: CursorPos) {
        let max_row = self.size.rows.saturating_sub(1);
        let max_col = self.size.cols.saturating_sub(1);
        self.cursor = CursorPos {
            row: pos.row.min(max_row),
            col: pos.col.min(max_col),
        };
    }

    /// Return a copy of the current text attributes.
    pub fn current_attrs(&self) -> CellAttrs {
        self.current_attrs
    }

    /// Update the current text attributes.
    pub fn set_attrs(&mut self, attrs: CellAttrs) {
        self.current_attrs = attrs;
    }

    /// Return the window title if one has been set.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Scroll content up by `n` rows: remove top `n` rows, append `n` blank rows at bottom.
    pub fn scroll_up(&mut self, n: usize) {
        let n = n.min(self.size.rows);
        if n == 0 {
            return;
        }
        self.cells.drain(0..n);
        for _ in 0..n {
            self.cells
                .push((0..self.size.cols).map(|_| Cell::default()).collect());
        }
        self.dirty = true;
    }

    /// Scroll content down by `n` rows: remove bottom `n` rows, insert `n` blank rows at top.
    pub fn scroll_down(&mut self, n: usize) {
        let n = n.min(self.size.rows);
        if n == 0 {
            return;
        }
        self.cells.truncate(self.size.rows - n);
        for _ in 0..n {
            self.cells
                .insert(0, (0..self.size.cols).map(|_| Cell::default()).collect());
        }
        self.dirty = true;
    }

    /// Write a character at the cursor, applying current_attrs, then advance.
    ///
    /// If advancing past the last column: move to the start of the next row.
    /// If the next row is past the last row: scroll the grid up one row.
    pub fn put_char_at_cursor(&mut self, c: char) {
        let row = self.cursor.row;
        let col = self.cursor.col;
        // Clamp in case cursor was somehow out of range.
        let row = row.min(self.size.rows.saturating_sub(1));
        let col = col.min(self.size.cols.saturating_sub(1));

        {
            let cell = &mut self.cells[row][col];
            cell.c = c;
            cell.attrs = self.current_attrs;
            cell.dirty = true;
        }
        self.dirty = true;

        // Advance cursor.
        let next_col = col + 1;
        if next_col >= self.size.cols {
            // Wrap to next row.
            let next_row = row + 1;
            if next_row >= self.size.rows {
                // Scroll up, stay on last row.
                self.scroll_up(1);
                self.cursor.row = self.size.rows.saturating_sub(1);
            } else {
                self.cursor.row = next_row;
            }
            self.cursor.col = 0;
        } else {
            self.cursor.col = next_col;
        }
    }

    /// Resize the grid, preserving content where possible.
    pub fn resize(&mut self, new_size: GridSize) {
        let mut new_cells: Vec<Vec<Cell>> = (0..new_size.rows)
            .map(|_| (0..new_size.cols).map(|_| Cell::default()).collect())
            .collect();

        let copy_rows = self.size.rows.min(new_size.rows);
        let copy_cols = self.size.cols.min(new_size.cols);
        for (r, new_row) in new_cells.iter_mut().enumerate().take(copy_rows) {
            new_row[..copy_cols].clone_from_slice(&self.cells[r][..copy_cols]);
        }

        self.cells = new_cells;
        self.size = new_size;
        self.dirty = true;

        // Clamp cursor to new bounds.
        if self.cursor.row >= new_size.rows {
            self.cursor.row = new_size.rows.saturating_sub(1);
        }
        if self.cursor.col >= new_size.cols {
            self.cursor.col = new_size.cols.saturating_sub(1);
        }
    }

    /// Reset all cells and cursor to defaults, marking the grid dirty.
    pub fn clear(&mut self) {
        for row in &mut self.cells {
            for cell in row {
                *cell = Cell::default();
            }
        }
        self.cursor = CursorPos::default();
        self.dirty = true;
    }

    /// Mark the grid and all cells as clean.
    pub fn mark_clean(&mut self) {
        self.dirty = false;
        for row in &mut self.cells {
            for cell in row {
                cell.dirty = false;
            }
        }
    }

    /// Return all rows as a slice.
    pub fn rows(&self) -> &[Vec<Cell>] {
        &self.cells
    }

    // -------------------------------------------------------------------------
    // SGR (Select Graphic Rendition) helper — processes a flat param slice.
    // This lives on Grid so that the arcterm-vt Handler impl can delegate here.
    // -------------------------------------------------------------------------

    /// Apply SGR parameters to `current_attrs`.
    ///
    /// Params are provided as a flat `&[u16]` slice (the raw parameter values,
    /// with extended colors like `38;5;N` expressed as three consecutive values).
    pub fn apply_sgr(&mut self, params: &[u16]) {
        let mut i = 0;
        while i < params.len() {
            match params[i] {
                0 => {
                    self.current_attrs = CellAttrs::default();
                }
                1 => self.current_attrs.bold = true,
                3 => self.current_attrs.italic = true,
                4 => self.current_attrs.underline = true,
                7 => self.current_attrs.reverse = true,
                // Foreground colors 30-37 → Indexed(0-7)
                n @ 30..=37 => self.current_attrs.fg = Color::Indexed((n - 30) as u8),
                // Default foreground
                39 => self.current_attrs.fg = Color::Default,
                // Background colors 40-47 → Indexed(0-7)
                n @ 40..=47 => self.current_attrs.bg = Color::Indexed((n - 40) as u8),
                // Default background
                49 => self.current_attrs.bg = Color::Default,
                // Bright foreground 90-97 → Indexed(8-15)
                n @ 90..=97 => self.current_attrs.fg = Color::Indexed((n - 90 + 8) as u8),
                // Bright background 100-107 → Indexed(8-15)
                n @ 100..=107 => self.current_attrs.bg = Color::Indexed((n - 100 + 8) as u8),
                // 256-color / RGB foreground
                38 => {
                    if i + 1 < params.len() {
                        match params[i + 1] {
                            5 if i + 2 < params.len() => {
                                self.current_attrs.fg = Color::Indexed(params[i + 2] as u8);
                                i += 2;
                            }
                            2 if i + 4 < params.len() => {
                                self.current_attrs.fg = Color::Rgb(
                                    params[i + 2] as u8,
                                    params[i + 3] as u8,
                                    params[i + 4] as u8,
                                );
                                i += 4;
                            }
                            _ => {}
                        }
                    }
                }
                // 256-color / RGB background
                48 => {
                    if i + 1 < params.len() {
                        match params[i + 1] {
                            5 if i + 2 < params.len() => {
                                self.current_attrs.bg = Color::Indexed(params[i + 2] as u8);
                                i += 2;
                            }
                            2 if i + 4 < params.len() => {
                                self.current_attrs.bg = Color::Rgb(
                                    params[i + 2] as u8,
                                    params[i + 3] as u8,
                                    params[i + 4] as u8,
                                );
                                i += 4;
                            }
                            _ => {}
                        }
                    }
                }
                _ => {} // unknown/unimplemented SGR codes — silently ignored
            }
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gridsize_new_stores_dimensions() {
        let gs = GridSize::new(24, 80);
        assert_eq!(gs.rows, 24);
        assert_eq!(gs.cols, 80);
    }

    #[test]
    fn grid_new_creates_correct_dimensions() {
        let g = Grid::new(GridSize::new(10, 20));
        assert_eq!(g.size.rows, 10);
        assert_eq!(g.size.cols, 20);
        assert_eq!(g.rows().len(), 10);
        assert_eq!(g.rows()[0].len(), 20);
    }

    #[test]
    fn grid_new_cursor_at_origin() {
        let g = Grid::new(GridSize::new(5, 5));
        assert_eq!(g.cursor, CursorPos { row: 0, col: 0 });
    }

    #[test]
    fn grid_new_is_dirty() {
        let g = Grid::new(GridSize::new(2, 2));
        assert!(g.dirty);
    }

    #[test]
    fn grid_cell_access() {
        let g = Grid::new(GridSize::new(3, 3));
        let c = g.cell(0, 0);
        assert_eq!(c.c, ' ');
    }

    #[test]
    fn grid_cell_mut_marks_dirty() {
        let mut g = Grid::new(GridSize::new(3, 3));
        g.mark_clean();
        assert!(!g.dirty);
        let cell = g.cell_mut(1, 1);
        cell.set_char('X');
        assert!(g.dirty, "cell_mut must mark the grid dirty");
    }

    #[test]
    fn grid_resize_preserves_content() {
        let mut g = Grid::new(GridSize::new(3, 3));
        g.cell_mut(0, 0).set_char('A');
        g.cell_mut(2, 2).set_char('B');
        g.resize(GridSize::new(5, 5));
        assert_eq!(g.cell(0, 0).c, 'A', "content at (0,0) must survive resize");
        assert_eq!(g.cell(2, 2).c, 'B', "content at (2,2) must survive resize");
        assert_eq!(g.size.rows, 5);
        assert_eq!(g.size.cols, 5);
    }

    #[test]
    fn grid_resize_shrink() {
        let mut g = Grid::new(GridSize::new(5, 5));
        g.cell_mut(0, 0).set_char('A');
        g.resize(GridSize::new(2, 2));
        assert_eq!(g.size.rows, 2);
        assert_eq!(g.size.cols, 2);
        assert_eq!(g.cell(0, 0).c, 'A');
    }

    #[test]
    fn grid_clear_resets_all_cells() {
        let mut g = Grid::new(GridSize::new(3, 3));
        g.cell_mut(1, 1).set_char('Z');
        g.cursor = CursorPos { row: 2, col: 2 };
        g.clear();
        assert_eq!(g.cell(1, 1).c, ' ', "clear must reset cell characters");
        assert_eq!(
            g.cursor,
            CursorPos { row: 0, col: 0 },
            "clear must reset cursor"
        );
        assert!(g.dirty, "clear must mark grid dirty");
    }

    #[test]
    fn grid_mark_clean() {
        let mut g = Grid::new(GridSize::new(2, 2));
        assert!(g.dirty);
        g.mark_clean();
        assert!(!g.dirty);
        for row in g.rows() {
            for cell in row {
                assert!(!cell.dirty, "mark_clean must clear all cell dirty flags");
            }
        }
    }

    #[test]
    fn cursorpos_default_is_origin() {
        let cp = CursorPos::default();
        assert_eq!(cp.row, 0);
        assert_eq!(cp.col, 0);
    }

    #[test]
    fn grid_derives_debug_clone_partialeq() {
        let g1 = Grid::new(GridSize::new(2, 2));
        let g2 = g1.clone();
        assert_eq!(g1, g2);
        let _ = format!("{:?}", g1);
    }

    #[test]
    fn set_cursor_clamps_to_bounds() {
        let mut g = Grid::new(GridSize::new(5, 10));
        g.set_cursor(CursorPos {
            row: 100,
            col: 200,
        });
        assert_eq!(g.cursor(), CursorPos { row: 4, col: 9 });
    }

    #[test]
    fn scroll_up_shifts_rows() {
        let mut g = Grid::new(GridSize::new(3, 3));
        g.cell_mut(0, 0).set_char('A');
        g.cell_mut(1, 0).set_char('B');
        g.cell_mut(2, 0).set_char('C');
        g.scroll_up(1);
        assert_eq!(g.cell(0, 0).c, 'B');
        assert_eq!(g.cell(1, 0).c, 'C');
        assert_eq!(g.cell(2, 0).c, ' ');
    }

    #[test]
    fn scroll_down_shifts_rows() {
        let mut g = Grid::new(GridSize::new(3, 3));
        g.cell_mut(0, 0).set_char('A');
        g.cell_mut(1, 0).set_char('B');
        g.cell_mut(2, 0).set_char('C');
        g.scroll_down(1);
        assert_eq!(g.cell(0, 0).c, ' ');
        assert_eq!(g.cell(1, 0).c, 'A');
        assert_eq!(g.cell(2, 0).c, 'B');
    }

    #[test]
    fn put_char_at_cursor_writes_and_advances() {
        let mut g = Grid::new(GridSize::new(5, 10));
        g.put_char_at_cursor('Z');
        assert_eq!(g.cell(0, 0).c, 'Z');
        assert_eq!(g.cursor(), CursorPos { row: 0, col: 1 });
    }

    #[test]
    fn put_char_at_cursor_wraps_at_end_of_row() {
        let mut g = Grid::new(GridSize::new(5, 3));
        g.put_char_at_cursor('a');
        g.put_char_at_cursor('b');
        g.put_char_at_cursor('c'); // fills col 2, wraps to next row
        assert_eq!(g.cursor(), CursorPos { row: 1, col: 0 });
    }

    #[test]
    fn put_char_at_cursor_scrolls_at_bottom() {
        let mut g = Grid::new(GridSize::new(2, 2));
        g.put_char_at_cursor('a'); // (0,0)
        g.put_char_at_cursor('b'); // (0,1) -> wraps to (1,0)
        g.put_char_at_cursor('c'); // (1,0)
        g.put_char_at_cursor('d'); // (1,1) -> wraps; at bottom, scroll up
        // Now cursor should be at (1,0) after scroll
        g.put_char_at_cursor('e'); // writes at (1,0) of scrolled grid
        assert_eq!(g.cell(1, 0).c, 'e');
    }

    #[test]
    fn apply_sgr_bold() {
        let mut g = Grid::new(GridSize::new(5, 10));
        g.apply_sgr(&[1]);
        assert!(g.current_attrs.bold);
    }

    #[test]
    fn apply_sgr_reset() {
        let mut g = Grid::new(GridSize::new(5, 10));
        g.apply_sgr(&[1]);
        g.apply_sgr(&[0]);
        assert!(!g.current_attrs.bold);
    }

    #[test]
    fn apply_sgr_fg_color() {
        let mut g = Grid::new(GridSize::new(5, 10));
        g.apply_sgr(&[31]);
        assert_eq!(g.current_attrs.fg, Color::Indexed(1));
    }

    #[test]
    fn apply_sgr_256_color_fg() {
        let mut g = Grid::new(GridSize::new(5, 10));
        g.apply_sgr(&[38, 5, 196]);
        assert_eq!(g.current_attrs.fg, Color::Indexed(196));
    }

    #[test]
    fn apply_sgr_rgb_fg() {
        let mut g = Grid::new(GridSize::new(5, 10));
        g.apply_sgr(&[38, 2, 255, 128, 0]);
        assert_eq!(g.current_attrs.fg, Color::Rgb(255, 128, 0));
    }
}
