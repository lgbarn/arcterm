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
