//! Terminal grid and cursor types.

use crate::cell::Cell;

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
pub struct Grid {
    pub cells: Vec<Vec<Cell>>,
    pub size: GridSize,
    pub cursor: CursorPos,
    pub dirty: bool,
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
        }
    }

    /// Immutable cell access (panics on out-of-bounds).
    pub fn cell(&self, row: usize, col: usize) -> &Cell {
        &self.cells[row][col]
    }

    /// Mutable cell access; marks the grid dirty.
    pub fn cell_mut(&mut self, row: usize, col: usize) -> &mut Cell {
        self.dirty = true;
        &mut self.cells[row][col]
    }

    /// Resize the grid, preserving content where possible.
    pub fn resize(&mut self, new_size: GridSize) {
        let mut new_cells: Vec<Vec<Cell>> = (0..new_size.rows)
            .map(|_| (0..new_size.cols).map(|_| Cell::default()).collect())
            .collect();

        let copy_rows = self.size.rows.min(new_size.rows);
        let copy_cols = self.size.cols.min(new_size.cols);
        for r in 0..copy_rows {
            for c in 0..copy_cols {
                new_cells[r][c] = self.cells[r][c].clone();
            }
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
        assert_eq!(g.cursor, CursorPos { row: 0, col: 0 }, "clear must reset cursor");
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
}
