//! Handler trait for semantic terminal operations, plus the Grid implementation.

use arcterm_core::{CursorPos, Grid};

/// Semantic terminal operations. All methods have default no-op implementations
/// so implementations only override what they need.
pub trait Handler {
    /// Write a character at the cursor position and advance the cursor.
    fn put_char(&mut self, _c: char) {}

    /// Move cursor down one row; scroll the grid if at the bottom row.
    fn newline(&mut self) {}

    /// Move cursor to column 0.
    fn carriage_return(&mut self) {}

    /// Move cursor left one column (minimum column 0).
    fn backspace(&mut self) {}

    /// Advance cursor to the next tab stop (every 8 columns).
    fn tab(&mut self) {}

    /// Bell — no-op for Phase 1.
    fn bell(&mut self) {}

    /// Set cursor position (0-indexed, bounds-clamped).
    fn set_cursor_pos(&mut self, _row: usize, _col: usize) {}

    /// Move cursor up by n rows.
    fn cursor_up(&mut self, _n: usize) {}

    /// Move cursor down by n rows.
    fn cursor_down(&mut self, _n: usize) {}

    /// Move cursor forward (right) by n columns.
    fn cursor_forward(&mut self, _n: usize) {}

    /// Move cursor backward (left) by n columns.
    fn cursor_backward(&mut self, _n: usize) {}

    /// Erase in display: 0=below cursor, 1=above cursor, 2=all.
    fn erase_in_display(&mut self, _mode: u16) {}

    /// Erase in line: 0=from cursor to end, 1=from start to cursor, 2=entire line.
    fn erase_in_line(&mut self, _mode: u16) {}

    /// Apply SGR (Select Graphic Rendition) parameters.
    fn set_sgr(&mut self, _params: &[u16]) {}

    /// Scroll grid content up by n rows (new blank rows at bottom).
    fn scroll_up(&mut self, _n: usize) {}

    /// Scroll grid content down by n rows (new blank rows at top).
    fn scroll_down(&mut self, _n: usize) {}

    /// Line feed (0x0A).
    fn line_feed(&mut self) {}

    /// Store the window title (OSC 0/2).
    fn set_title(&mut self, _title: &str) {}
}

// ---------------------------------------------------------------------------
// Handler implementation for Grid
// ---------------------------------------------------------------------------

impl Handler for Grid {
    fn put_char(&mut self, c: char) {
        self.put_char_at_cursor(c);
    }

    fn newline(&mut self) {
        // newline = move down, scroll if needed (does NOT reset column)
        let next_row = self.cursor().row + 1;
        if next_row >= self.size.rows {
            self.scroll_up(1);
            // cursor stays at last row after scroll
            let last = self.size.rows.saturating_sub(1);
            self.set_cursor(CursorPos {
                row: last,
                col: self.cursor().col,
            });
        } else {
            self.set_cursor(CursorPos {
                row: next_row,
                col: self.cursor().col,
            });
        }
    }

    fn carriage_return(&mut self) {
        let row = self.cursor().row;
        self.set_cursor(CursorPos { row, col: 0 });
    }

    fn backspace(&mut self) {
        let cur = self.cursor();
        self.set_cursor(CursorPos {
            row: cur.row,
            col: cur.col.saturating_sub(1),
        });
    }

    fn tab(&mut self) {
        let cur = self.cursor();
        let next_stop = (cur.col / 8 + 1) * 8;
        let max_col = self.size.cols.saturating_sub(1);
        self.set_cursor(CursorPos {
            row: cur.row,
            col: next_stop.min(max_col),
        });
    }

    fn bell(&mut self) {
        // no-op for Phase 1
    }

    fn set_cursor_pos(&mut self, row: usize, col: usize) {
        self.set_cursor(CursorPos { row, col });
    }

    fn cursor_up(&mut self, n: usize) {
        let cur = self.cursor();
        self.set_cursor(CursorPos {
            row: cur.row.saturating_sub(n),
            col: cur.col,
        });
    }

    fn cursor_down(&mut self, n: usize) {
        let cur = self.cursor();
        self.set_cursor(CursorPos {
            row: cur.row.saturating_add(n),
            col: cur.col,
        });
    }

    fn cursor_forward(&mut self, n: usize) {
        let cur = self.cursor();
        self.set_cursor(CursorPos {
            row: cur.row,
            col: cur.col.saturating_add(n),
        });
    }

    fn cursor_backward(&mut self, n: usize) {
        let cur = self.cursor();
        self.set_cursor(CursorPos {
            row: cur.row,
            col: cur.col.saturating_sub(n),
        });
    }

    fn erase_in_display(&mut self, mode: u16) {
        let (rows, cols) = (self.size.rows, self.size.cols);
        let cur = self.cursor();
        match mode {
            0 => {
                // Erase from cursor to end of display (inclusive of cursor cell)
                for c in cur.col..cols {
                    self.cells[cur.row][c].reset();
                }
                for r in (cur.row + 1)..rows {
                    for c in 0..cols {
                        self.cells[r][c].reset();
                    }
                }
            }
            1 => {
                // Erase from top of display to cursor (inclusive)
                for r in 0..cur.row {
                    for c in 0..cols {
                        self.cells[r][c].reset();
                    }
                }
                for c in 0..=cur.col.min(cols.saturating_sub(1)) {
                    self.cells[cur.row][c].reset();
                }
            }
            2 | 3 => {
                // Erase entire display
                for r in 0..rows {
                    for c in 0..cols {
                        self.cells[r][c].reset();
                    }
                }
            }
            _ => {}
        }
        self.dirty = true;
    }

    fn erase_in_line(&mut self, mode: u16) {
        let cols = self.size.cols;
        let cur = self.cursor();
        match mode {
            0 => {
                // Erase from cursor to end of line (inclusive)
                for c in cur.col..cols {
                    self.cells[cur.row][c].reset();
                }
            }
            1 => {
                // Erase from start of line to cursor (inclusive)
                for c in 0..=cur.col.min(cols.saturating_sub(1)) {
                    self.cells[cur.row][c].reset();
                }
            }
            2 => {
                // Erase entire line
                for c in 0..cols {
                    self.cells[cur.row][c].reset();
                }
            }
            _ => {}
        }
        self.dirty = true;
    }

    fn set_sgr(&mut self, params: &[u16]) {
        self.apply_sgr(params);
    }

    fn scroll_up(&mut self, n: usize) {
        self.scroll_up(n);
    }

    fn scroll_down(&mut self, n: usize) {
        self.scroll_down(n);
    }

    fn line_feed(&mut self) {
        // line_feed is identical to newline for Phase 1
        Handler::newline(self);
    }

    fn set_title(&mut self, title: &str) {
        self.title = Some(title.to_string());
    }
}
