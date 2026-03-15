//! Handler trait for semantic terminal operations, plus the Grid implementation.

use arcterm_core::{Cell, CursorPos, Grid};

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

    // -------------------------------------------------------------------------
    // DEC Private Modes and Mode control (Phase 2)
    // -------------------------------------------------------------------------

    /// Set a terminal mode. `private` = true for DEC private modes (ESC[?...h).
    fn set_mode(&mut self, _mode: u16, _private: bool) {}

    /// Reset a terminal mode. `private` = true for DEC private modes (ESC[?...l).
    fn reset_mode(&mut self, _mode: u16, _private: bool) {}

    // -------------------------------------------------------------------------
    // Scroll region (DECSTBM)
    // -------------------------------------------------------------------------

    /// Set scrolling region: top and bottom are 0-indexed row numbers.
    fn set_scroll_region(&mut self, _top: usize, _bottom: usize) {}

    // -------------------------------------------------------------------------
    // Cursor save/restore (DECSC/DECRC, ESC 7/8)
    // -------------------------------------------------------------------------

    /// Save the current cursor position.
    fn save_cursor_position(&mut self) {}

    /// Restore the previously saved cursor position.
    fn restore_cursor_position(&mut self) {}

    // -------------------------------------------------------------------------
    // Line editing (IL/DL)
    // -------------------------------------------------------------------------

    /// Insert n blank lines at the cursor row, pushing lines down.
    fn insert_lines(&mut self, _n: usize) {}

    /// Delete n lines at the cursor row, pulling lines up.
    fn delete_lines(&mut self, _n: usize) {}

    // -------------------------------------------------------------------------
    // Character editing (ICH/DCH/ECH)
    // -------------------------------------------------------------------------

    /// Insert n blank characters at the cursor column, pushing characters right.
    fn insert_chars(&mut self, _n: usize) {}

    /// Delete n characters at the cursor column, pulling characters left.
    fn delete_chars(&mut self, _n: usize) {}

    /// Erase n characters starting at the cursor column (replace with spaces).
    fn erase_chars(&mut self, _n: usize) {}

    // -------------------------------------------------------------------------
    // Cursor absolute positioning (CHA/VPA)
    // -------------------------------------------------------------------------

    /// Move cursor to absolute column (0-indexed).
    fn cursor_horizontal_absolute(&mut self, _col: usize) {}

    /// Move cursor to absolute row (0-indexed).
    fn cursor_vertical_absolute(&mut self, _row: usize) {}

    // -------------------------------------------------------------------------
    // Device reports (DSR/DA)
    // -------------------------------------------------------------------------

    /// Device Status Report: respond to DSR request (param n).
    fn device_status_report(&mut self, _n: u16) {}

    /// Send Primary Device Attributes response.
    fn device_attributes(&mut self) {}

    // -------------------------------------------------------------------------
    // Keypad mode
    // -------------------------------------------------------------------------

    /// Enter application keypad mode (ESC =).
    fn set_keypad_application_mode(&mut self) {}

    /// Return to numeric keypad mode (ESC >).
    fn set_keypad_numeric_mode(&mut self) {}
}

// ---------------------------------------------------------------------------
// TermModes — tracks active terminal mode flags
// ---------------------------------------------------------------------------

/// Active terminal mode flags stored on the Grid.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TermModes {
    pub cursor_visible: bool,
    pub auto_wrap: bool,
    pub app_cursor_keys: bool,
    pub alt_screen: bool,
    pub bracketed_paste: bool,
    pub app_keypad: bool,
    /// Mode 1000 — report mouse button press/release events.
    pub mouse_report_click: bool,
    /// Mode 1002 — report mouse button events and motion while a button is held.
    pub mouse_report_button: bool,
    /// Mode 1003 — report all mouse motion events.
    pub mouse_report_any: bool,
    /// Mode 1006 — use SGR (1006) extended mouse coordinate encoding.
    pub mouse_sgr_ext: bool,
}

impl TermModes {
    pub fn new() -> Self {
        Self {
            cursor_visible: true,
            auto_wrap: true,
            app_cursor_keys: false,
            alt_screen: false,
            bracketed_paste: false,
            app_keypad: false,
            mouse_report_click: false,
            mouse_report_button: false,
            mouse_report_any: false,
            mouse_sgr_ext: false,
        }
    }
}

// ---------------------------------------------------------------------------
// GridState — extends Grid with Phase 2 state managed in arcterm-vt
// ---------------------------------------------------------------------------

/// Wrapper that adds Phase 2 state (scroll region, saved cursor, modes)
/// around an arcterm-core Grid. This avoids modifying arcterm-core during
/// parallel development.
pub struct GridState {
    pub grid: Grid,
    pub modes: TermModes,
    /// Scroll region: (top_row, bottom_row), both 0-indexed, inclusive.
    pub scroll_top: usize,
    pub scroll_bottom: usize,
    /// Saved cursor position for DECSC/DECRC.
    pub saved_cursor: Option<CursorPos>,
    /// Normal-screen grid saved when entering alt screen.
    pub normal_screen: Option<Grid>,
}

impl GridState {
    pub fn new(grid: Grid) -> Self {
        let bottom = grid.size.rows.saturating_sub(1);
        Self {
            grid,
            modes: TermModes::new(),
            scroll_top: 0,
            scroll_bottom: bottom,
            saved_cursor: None,
            normal_screen: None,
        }
    }

    /// Effective scroll bottom, clamped to grid dimensions.
    fn eff_scroll_bottom(&self) -> usize {
        self.scroll_bottom.min(self.grid.size.rows.saturating_sub(1))
    }

    /// Effective scroll top.
    fn eff_scroll_top(&self) -> usize {
        self.scroll_top
    }

    // -----------------------------------------------------------------------
    // Region-aware scroll helpers
    // -----------------------------------------------------------------------

    /// Scroll the scroll region up by n rows.
    fn scroll_region_up(&mut self, n: usize) {
        let top = self.eff_scroll_top();
        let bottom = self.eff_scroll_bottom();
        let region_height = bottom + 1 - top;
        let n = n.min(region_height);
        if n == 0 {
            return;
        }
        let cols = self.grid.size.cols;
        // Shift rows up within the region.
        for row in top..=(bottom - n) {
            for col in 0..cols {
                self.grid.cells[row][col] = self.grid.cells[row + n][col].clone();
            }
        }
        // Clear the vacated rows at the bottom of the region.
        for row in (bottom + 1 - n)..=(bottom) {
            for col in 0..cols {
                self.grid.cells[row][col] = Cell::default();
            }
        }
        self.grid.dirty = true;
    }

    /// Scroll the scroll region down by n rows.
    fn scroll_region_down(&mut self, n: usize) {
        let top = self.eff_scroll_top();
        let bottom = self.eff_scroll_bottom();
        let region_height = bottom + 1 - top;
        let n = n.min(region_height);
        if n == 0 {
            return;
        }
        let cols = self.grid.size.cols;
        // Shift rows down within the region.
        for row in (top + n..=bottom).rev() {
            for col in 0..cols {
                self.grid.cells[row][col] = self.grid.cells[row - n][col].clone();
            }
        }
        // Clear the vacated rows at the top of the region.
        for row in top..(top + n) {
            for col in 0..cols {
                self.grid.cells[row][col] = Cell::default();
            }
        }
        self.grid.dirty = true;
    }
}

// ---------------------------------------------------------------------------
// Handler implementation for GridState
// ---------------------------------------------------------------------------

impl Handler for GridState {
    fn put_char(&mut self, c: char) {
        self.grid.put_char_at_cursor(c);
    }

    fn newline(&mut self) {
        let cur_row = self.grid.cursor().row;
        let scroll_bottom = self.eff_scroll_bottom();

        if cur_row >= scroll_bottom {
            // At or past the bottom of the scroll region — scroll the region up.
            self.scroll_region_up(1);
            // Cursor stays pinned at scroll_bottom row.
            self.grid.set_cursor(CursorPos {
                row: scroll_bottom,
                col: self.grid.cursor().col,
            });
        } else {
            // Cursor is above the scroll region bottom: move down one row freely.
            // A cursor above the scroll region top moves toward the region without
            // triggering a scroll; once inside the region it scrolls at the bottom.
            self.grid.set_cursor(CursorPos {
                row: cur_row + 1,
                col: self.grid.cursor().col,
            });
        }
    }

    fn carriage_return(&mut self) {
        let row = self.grid.cursor().row;
        self.grid.set_cursor(CursorPos { row, col: 0 });
    }

    fn backspace(&mut self) {
        let cur = self.grid.cursor();
        self.grid.set_cursor(CursorPos {
            row: cur.row,
            col: cur.col.saturating_sub(1),
        });
    }

    fn tab(&mut self) {
        let cur = self.grid.cursor();
        let next_stop = (cur.col / 8 + 1) * 8;
        let max_col = self.grid.size.cols.saturating_sub(1);
        self.grid.set_cursor(CursorPos {
            row: cur.row,
            col: next_stop.min(max_col),
        });
    }

    fn bell(&mut self) {}

    fn set_cursor_pos(&mut self, row: usize, col: usize) {
        self.grid.set_cursor(CursorPos { row, col });
    }

    fn cursor_up(&mut self, n: usize) {
        let cur = self.grid.cursor();
        self.grid.set_cursor(CursorPos {
            row: cur.row.saturating_sub(n),
            col: cur.col,
        });
    }

    fn cursor_down(&mut self, n: usize) {
        let cur = self.grid.cursor();
        let new_row = (cur.row + n).min(self.grid.size.rows.saturating_sub(1));
        self.grid.set_cursor(CursorPos {
            row: new_row,
            col: cur.col,
        });
    }

    fn cursor_forward(&mut self, n: usize) {
        let cur = self.grid.cursor();
        let new_col = (cur.col + n).min(self.grid.size.cols.saturating_sub(1));
        self.grid.set_cursor(CursorPos {
            row: cur.row,
            col: new_col,
        });
    }

    fn cursor_backward(&mut self, n: usize) {
        let cur = self.grid.cursor();
        self.grid.set_cursor(CursorPos {
            row: cur.row,
            col: cur.col.saturating_sub(n),
        });
    }

    fn erase_in_display(&mut self, mode: u16) {
        let (rows, cols) = (self.grid.size.rows, self.grid.size.cols);
        let cur = self.grid.cursor();
        match mode {
            0 => {
                for c in cur.col..cols {
                    self.grid.cells[cur.row][c] = Cell::default();
                }
                for r in (cur.row + 1)..rows {
                    for c in 0..cols {
                        self.grid.cells[r][c] = Cell::default();
                    }
                }
            }
            1 => {
                for r in 0..cur.row {
                    for c in 0..cols {
                        self.grid.cells[r][c] = Cell::default();
                    }
                }
                for c in 0..=cur.col.min(cols.saturating_sub(1)) {
                    self.grid.cells[cur.row][c] = Cell::default();
                }
            }
            2 | 3 => {
                for r in 0..rows {
                    for c in 0..cols {
                        self.grid.cells[r][c] = Cell::default();
                    }
                }
            }
            _ => {}
        }
        self.grid.dirty = true;
    }

    fn erase_in_line(&mut self, mode: u16) {
        let cols = self.grid.size.cols;
        let cur = self.grid.cursor();
        match mode {
            0 => {
                for c in cur.col..cols {
                    self.grid.cells[cur.row][c] = Cell::default();
                }
            }
            1 => {
                for c in 0..=cur.col.min(cols.saturating_sub(1)) {
                    self.grid.cells[cur.row][c] = Cell::default();
                }
            }
            2 => {
                for c in 0..cols {
                    self.grid.cells[cur.row][c] = Cell::default();
                }
            }
            _ => {}
        }
        self.grid.dirty = true;
    }

    fn set_sgr(&mut self, params: &[u16]) {
        self.grid.apply_sgr(params);
    }

    fn scroll_up(&mut self, n: usize) {
        self.scroll_region_up(n);
    }

    fn scroll_down(&mut self, n: usize) {
        self.scroll_region_down(n);
    }

    fn line_feed(&mut self) {
        Handler::newline(self);
    }

    fn set_title(&mut self, title: &str) {
        self.grid.title = Some(title.to_string());
    }

    // -------------------------------------------------------------------------
    // DEC Private Modes
    // -------------------------------------------------------------------------

    fn set_mode(&mut self, mode: u16, private: bool) {
        if private {
            match mode {
                1 => self.modes.app_cursor_keys = true,
                7 => self.modes.auto_wrap = true,
                25 => self.modes.cursor_visible = true,
                // Mode 47 and 1047: enter alt screen WITHOUT cursor save/restore.
                // Used by older applications and some tmux configurations.
                47 | 1047 => {
                    if !self.modes.alt_screen {
                        self.modes.alt_screen = true;
                        let saved = self.grid.clone();
                        self.normal_screen = Some(saved);
                        // Clear the current grid for the alt screen.
                        for row in &mut self.grid.cells {
                            for cell in row {
                                *cell = Cell::default();
                            }
                        }
                        self.grid.dirty = true;
                    }
                }
                // Mode 1049: enter alt screen WITH cursor save/restore.
                1049 => {
                    // Enter alt screen: save normal grid, clear alt screen.
                    if !self.modes.alt_screen {
                        self.modes.alt_screen = true;
                        let saved = self.grid.clone();
                        self.normal_screen = Some(saved);
                        // Clear the current grid for the alt screen.
                        for row in &mut self.grid.cells {
                            for cell in row {
                                *cell = Cell::default();
                            }
                        }
                        self.grid.cursor = CursorPos::default();
                        self.grid.dirty = true;
                    }
                }
                2004 => self.modes.bracketed_paste = true,
                // Mouse reporting modes — store flags for future use.
                1000 => self.modes.mouse_report_click = true,
                1002 => self.modes.mouse_report_button = true,
                1003 => self.modes.mouse_report_any = true,
                1006 => self.modes.mouse_sgr_ext = true,
                _ => {}
            }
        } else {
            // Standard modes (SM).
            // Mode 20 (LNM) etc. — currently no-op for unrecognized.
            let _ = mode;
        }
    }

    fn reset_mode(&mut self, mode: u16, private: bool) {
        if private {
            match mode {
                1 => self.modes.app_cursor_keys = false,
                7 => self.modes.auto_wrap = false,
                25 => self.modes.cursor_visible = false,
                // Mode 47 and 1047: leave alt screen WITHOUT cursor save/restore.
                47 | 1047 => {
                    if self.modes.alt_screen {
                        self.modes.alt_screen = false;
                        if let Some(saved) = self.normal_screen.take() {
                            self.grid = saved;
                        }
                    }
                }
                // Mode 1049: leave alt screen WITH cursor restore.
                1049 => {
                    // Leave alt screen: restore normal grid.
                    if self.modes.alt_screen {
                        self.modes.alt_screen = false;
                        if let Some(saved) = self.normal_screen.take() {
                            self.grid = saved;
                        }
                    }
                }
                2004 => self.modes.bracketed_paste = false,
                // Mouse reporting modes — clear flags.
                1000 => self.modes.mouse_report_click = false,
                1002 => self.modes.mouse_report_button = false,
                1003 => self.modes.mouse_report_any = false,
                1006 => self.modes.mouse_sgr_ext = false,
                _ => {}
            }
        } else {
            let _ = mode;
        }
    }

    // -------------------------------------------------------------------------
    // Scroll region (DECSTBM)
    // -------------------------------------------------------------------------

    fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        let max_row = self.grid.size.rows.saturating_sub(1);
        self.scroll_top = top.min(max_row);
        // usize::MAX sentinel means "default to last row".
        self.scroll_bottom = if bottom == usize::MAX {
            max_row
        } else {
            bottom.min(max_row)
        };
        // DECSTBM moves the cursor to the top-left corner.
        self.grid.set_cursor(CursorPos { row: 0, col: 0 });
    }

    // -------------------------------------------------------------------------
    // Cursor save/restore
    // -------------------------------------------------------------------------

    fn save_cursor_position(&mut self) {
        self.saved_cursor = Some(self.grid.cursor());
    }

    fn restore_cursor_position(&mut self) {
        if let Some(pos) = self.saved_cursor {
            self.grid.set_cursor(pos);
        }
    }

    // -------------------------------------------------------------------------
    // Line editing
    // -------------------------------------------------------------------------

    fn insert_lines(&mut self, n: usize) {
        let cur_row = self.grid.cursor().row;
        let bottom = self.eff_scroll_bottom();
        let cols = self.grid.size.cols;

        if cur_row > bottom {
            return;
        }

        let region_rows = bottom + 1 - cur_row;
        let n = n.min(region_rows);

        // Shift rows down within the region starting at cur_row.
        for row in (cur_row + n..=bottom).rev() {
            for col in 0..cols {
                self.grid.cells[row][col] = self.grid.cells[row - n][col].clone();
            }
        }
        // Clear the n rows at cur_row.
        for row in cur_row..(cur_row + n).min(bottom + 1) {
            for col in 0..cols {
                self.grid.cells[row][col] = Cell::default();
            }
        }
        self.grid.dirty = true;
    }

    fn delete_lines(&mut self, n: usize) {
        let cur_row = self.grid.cursor().row;
        let bottom = self.eff_scroll_bottom();
        let cols = self.grid.size.cols;

        if cur_row > bottom {
            return;
        }

        let region_rows = bottom + 1 - cur_row;
        let n = n.min(region_rows);

        // Shift rows up within the region starting at cur_row.
        for row in cur_row..=(bottom - n) {
            for col in 0..cols {
                self.grid.cells[row][col] = self.grid.cells[row + n][col].clone();
            }
        }
        // Clear the n rows at the bottom.
        for row in (bottom + 1 - n)..=bottom {
            for col in 0..cols {
                self.grid.cells[row][col] = Cell::default();
            }
        }
        self.grid.dirty = true;
    }

    // -------------------------------------------------------------------------
    // Character editing
    // -------------------------------------------------------------------------

    fn insert_chars(&mut self, n: usize) {
        let cur = self.grid.cursor();
        let cols = self.grid.size.cols;
        let row = cur.row;
        let col = cur.col;
        let n = n.min(cols - col);

        // Shift characters right by n positions, dropping those that go past the edge.
        for c in (col..cols - n).rev() {
            self.grid.cells[row][c + n] = self.grid.cells[row][c].clone();
        }
        // Clear the n cells at col.
        for c in col..(col + n) {
            self.grid.cells[row][c] = Cell::default();
        }
        self.grid.dirty = true;
    }

    fn delete_chars(&mut self, n: usize) {
        let cur = self.grid.cursor();
        let cols = self.grid.size.cols;
        let row = cur.row;
        let col = cur.col;
        let n = n.min(cols - col);

        // Shift characters left by n positions.
        for c in col..(cols - n) {
            self.grid.cells[row][c] = self.grid.cells[row][c + n].clone();
        }
        // Clear the n cells at the right end.
        for c in (cols - n)..cols {
            self.grid.cells[row][c] = Cell::default();
        }
        self.grid.dirty = true;
    }

    fn erase_chars(&mut self, n: usize) {
        let cur = self.grid.cursor();
        let cols = self.grid.size.cols;
        let row = cur.row;
        let col = cur.col;
        let end = (col + n).min(cols);
        for c in col..end {
            self.grid.cells[row][c] = Cell::default();
        }
        self.grid.dirty = true;
    }

    // -------------------------------------------------------------------------
    // Cursor absolute positioning
    // -------------------------------------------------------------------------

    fn cursor_horizontal_absolute(&mut self, col: usize) {
        let row = self.grid.cursor().row;
        self.grid.set_cursor(CursorPos { row, col });
    }

    fn cursor_vertical_absolute(&mut self, row: usize) {
        let col = self.grid.cursor().col;
        self.grid.set_cursor(CursorPos { row, col });
    }

    // -------------------------------------------------------------------------
    // Device reports — queue replies in pending_replies for PTY write-back.
    // -------------------------------------------------------------------------

    fn device_status_report(&mut self, n: u16) {
        if n == 6 {
            // DSR(6): report cursor position — ESC [ row ; col R (1-indexed).
            let row = self.grid.cursor().row + 1;
            let col = self.grid.cursor().col + 1;
            let reply = format!("\x1b[{};{}R", row, col).into_bytes();
            self.grid.pending_replies.push(reply);
        }
    }

    fn device_attributes(&mut self) {
        // Primary DA: report as VT100 with advanced video option.
        self.grid.pending_replies.push(b"\x1b[?1;2c".to_vec());
    }

    // -------------------------------------------------------------------------
    // Keypad mode
    // -------------------------------------------------------------------------

    fn set_keypad_application_mode(&mut self) {
        self.modes.app_keypad = true;
    }

    fn set_keypad_numeric_mode(&mut self) {
        self.modes.app_keypad = false;
    }
}

// ---------------------------------------------------------------------------
// Handler implementation for Grid (legacy / Phase 1 compatibility)
// ---------------------------------------------------------------------------

impl Handler for Grid {
    fn put_char(&mut self, c: char) {
        self.put_char_at_cursor(c);
    }

    fn newline(&mut self) {
        // Delegate to the scroll-region-aware method on Grid.
        self.newline_in_region();
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

// ---------------------------------------------------------------------------
// Tests — DSR/DA reply queuing on GridState
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use arcterm_core::{Grid, GridSize};

    fn make_grid_state() -> GridState {
        GridState::new(Grid::new(GridSize::new(24, 80)))
    }

    #[test]
    fn dsr_6_queues_cursor_position_reply() {
        let mut gs = make_grid_state();
        // Move cursor to a known position (0-indexed → 1-indexed in reply).
        gs.grid.set_cursor(CursorPos { row: 4, col: 9 });
        Handler::device_status_report(&mut gs, 6);
        assert_eq!(gs.grid.pending_replies.len(), 1);
        let reply = std::str::from_utf8(&gs.grid.pending_replies[0]).unwrap();
        assert_eq!(reply, "\x1b[5;10R", "DSR(6) reply must be 1-indexed row;col");
    }

    #[test]
    fn dsr_other_param_is_noop() {
        let mut gs = make_grid_state();
        Handler::device_status_report(&mut gs, 5);
        assert!(gs.grid.pending_replies.is_empty(), "DSR(5) must not queue a reply");
    }

    #[test]
    fn device_attributes_queues_da_reply() {
        let mut gs = make_grid_state();
        Handler::device_attributes(&mut gs);
        assert_eq!(gs.grid.pending_replies.len(), 1);
        let reply = std::str::from_utf8(&gs.grid.pending_replies[0]).unwrap();
        assert_eq!(reply, "\x1b[?1;2c", "DA reply must be ESC[?1;2c");
    }

    #[test]
    fn multiple_replies_accumulate() {
        let mut gs = make_grid_state();
        Handler::device_attributes(&mut gs);
        Handler::device_status_report(&mut gs, 6);
        assert_eq!(gs.grid.pending_replies.len(), 2, "both replies must be queued");
    }

    #[test]
    fn cursor_hidden_flag_can_be_toggled() {
        let mut gs = make_grid_state();
        assert!(gs.modes.cursor_visible, "cursor must be visible by default");
        Handler::reset_mode(&mut gs, 25, true);
        assert!(!gs.modes.cursor_visible, "mode reset 25 must hide cursor");
        Handler::set_mode(&mut gs, 25, true);
        assert!(gs.modes.cursor_visible, "mode set 25 must show cursor");
    }

    #[test]
    fn app_cursor_keys_toggled_via_mode() {
        let mut gs = make_grid_state();
        assert!(!gs.modes.app_cursor_keys);
        Handler::set_mode(&mut gs, 1, true);
        assert!(gs.modes.app_cursor_keys);
        Handler::reset_mode(&mut gs, 1, true);
        assert!(!gs.modes.app_cursor_keys);
    }
}
