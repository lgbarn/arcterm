//! Handler trait for semantic terminal operations, plus the Grid implementation.

use std::collections::HashMap;

use arcterm_core::{Cell, CursorPos, Grid, TermModes};

// ---------------------------------------------------------------------------
// ContentType — semantic classification for OSC 7770 structured content
// ---------------------------------------------------------------------------

/// Classifies the type of structured content carried in an OSC 7770 block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentType {
    CodeBlock,
    Diff,
    Plan,
    Markdown,
    Json,
    Error,
    Progress,
    Image,
}

// ---------------------------------------------------------------------------
// StructuredContentAccumulator — collects chars during an OSC 7770 block
// ---------------------------------------------------------------------------

/// Accumulates characters written inside an OSC 7770 `start` / `end` pair.
///
/// While an accumulator is active, every `put_char` call both writes to the
/// terminal grid (so the content is rendered normally) and appends to
/// `buffer` so the full text is available for structured processing.
#[derive(Debug, Clone)]
pub struct StructuredContentAccumulator {
    /// The semantic type of this content block.
    pub content_type: ContentType,
    /// Key/value attributes parsed from the OSC 7770 params (e.g. `lang=rust`).
    pub attrs: HashMap<String, String>,
    /// Raw text accumulated since the `start` OSC was received.
    pub buffer: String,
}

impl StructuredContentAccumulator {
    /// Create a new, empty accumulator for the given content type and attrs.
    pub fn new(content_type: ContentType, attrs: HashMap<String, String>) -> Self {
        Self {
            content_type,
            attrs,
            buffer: String::new(),
        }
    }
}

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

    // -------------------------------------------------------------------------
    // OSC 7770 — structured content (Phase 4)
    // -------------------------------------------------------------------------

    /// Begin a structured content block of the given type with optional attrs.
    fn structured_content_start(
        &mut self,
        _content_type: ContentType,
        _attrs: HashMap<String, String>,
    ) {
    }

    /// End the current structured content block and move it to completed.
    fn structured_content_end(&mut self) {}

    // -------------------------------------------------------------------------
    // Kitty graphics protocol (APC, Phase 4)
    // -------------------------------------------------------------------------

    /// Dispatch a Kitty Graphics Protocol command.
    ///
    /// `payload` is the raw bytes between the APC introducer (`ESC _`) and the
    /// String Terminator (`ESC \`), with the delimiters stripped.  The default
    /// implementation is a no-op so existing Handler implementors are unaffected.
    fn kitty_graphics_command(&mut self, _payload: &[u8]) {}

    // -------------------------------------------------------------------------
    // OSC 133 — shell integration (Phase 7)
    // -------------------------------------------------------------------------

    /// OSC 133 ; A — prompt start mark.
    fn shell_prompt_start(&mut self) {}

    /// OSC 133 ; B — command start mark (user has begun typing a command).
    fn shell_command_start(&mut self) {}

    /// OSC 133 ; D [; exit_code] — command end mark with optional exit code.
    ///
    /// `exit_code` defaults to 0 when the OSC sequence omits the code field.
    fn shell_command_end(&mut self, _exit_code: i32) {}

    // -------------------------------------------------------------------------
    // OSC 7770 — MCP tool discovery (Phase 7)
    // -------------------------------------------------------------------------

    /// Called when the VT processor receives `ESC ] 7770 ; tools/list ST`.
    ///
    /// Signals that the AI agent is querying available MCP tools.
    /// The app layer drains the resulting flag, calls `PluginManager::list_tools()`,
    /// and writes back an `ESC ] 7770 ; tools/response ; <base64_json> ST`.
    fn tool_list_query(&mut self) {}

    /// Called when the VT processor receives
    /// `ESC ] 7770 ; tools/call ; name=<n> ; args=<base64> ST`.
    ///
    /// `name` is the tool name; `args_json` is the decoded JSON arguments string.
    /// The app layer drains the call, invokes the tool, and writes back
    /// `ESC ] 7770 ; tools/result ; result=<base64_json> ST`.
    fn tool_call(&mut self, _name: String, _args_json: String) {}
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
    /// Active OSC 7770 accumulator, set between start and end signals.
    pub accumulator: Option<StructuredContentAccumulator>,
    /// Completed structured content blocks in receive order.
    pub completed_blocks: Vec<StructuredContentAccumulator>,
    /// Raw Kitty APC payloads received since the last drain.
    ///
    /// Each entry is the raw bytes between `ESC _` and `ESC \` (as delivered
    /// by `ApcScanner`).  The app layer drains these via
    /// [`GridState::take_kitty_payloads`] after each PTY processing batch.
    pub kitty_payloads: Vec<Vec<u8>>,
    /// Exit codes received via OSC 133 D since the last drain.
    ///
    /// The app layer drains these via [`GridState::take_exit_codes`] after
    /// each PTY processing batch and stores them in the per-pane `PaneContext`.
    pub shell_exit_codes: Vec<i32>,
    /// Set to `true` by `shell_command_start()` (OSC 133 B). Cleared when
    /// the corresponding `shell_command_end()` (OSC 133 D) is received.
    pub pending_command_start: bool,
    /// Drain buffer for `tools/list` queries (OSC 7770 ; tools/list).
    ///
    /// Each `()` entry represents one pending tool-list query from an AI agent.
    /// The app layer drains these, calls `PluginManager::list_tools()`, and
    /// writes the response back to the PTY.
    pub tool_queries: Vec<()>,
    /// Drain buffer for `tools/call` invocations (OSC 7770 ; tools/call).
    ///
    /// Each entry is `(tool_name, decoded_args_json)`.
    pub tool_calls: Vec<(String, String)>,
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
            accumulator: None,
            completed_blocks: Vec::new(),
            kitty_payloads: Vec::new(),
            shell_exit_codes: Vec::new(),
            pending_command_start: false,
            tool_queries: Vec::new(),
            tool_calls: Vec::new(),
        }
    }

    /// Drain and return all Kitty APC payloads received since the last call.
    pub fn take_kitty_payloads(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.kitty_payloads)
    }

    /// Drain and return all shell exit codes received via OSC 133 D since the
    /// last call. The caller (app layer) stores the last value in `PaneContext`.
    pub fn take_exit_codes(&mut self) -> Vec<i32> {
        std::mem::take(&mut self.shell_exit_codes)
    }

    /// Drain and return all pending tool-list queries (one `()` per query).
    pub fn take_tool_queries(&mut self) -> Vec<()> {
        std::mem::take(&mut self.tool_queries)
    }

    /// Drain and return all pending tool calls as `(name, args_json)` pairs.
    pub fn take_tool_calls(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.tool_calls)
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
        // Always render to the grid.
        self.grid.put_char_at_cursor(c);
        // If a structured content block is active, also append to its buffer.
        if let Some(acc) = self.accumulator.as_mut() {
            acc.buffer.push(c);
        }
    }

    fn structured_content_start(
        &mut self,
        content_type: ContentType,
        attrs: HashMap<String, String>,
    ) {
        self.accumulator = Some(StructuredContentAccumulator::new(content_type, attrs));
    }

    fn structured_content_end(&mut self) {
        if let Some(acc) = self.accumulator.take() {
            self.completed_blocks.push(acc);
        }
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
                25 => {
                    self.modes.cursor_visible = true;
                    self.grid.modes.cursor_visible = true;
                }
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
                25 => {
                    self.modes.cursor_visible = false;
                    self.grid.modes.cursor_visible = false;
                }
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

    // -------------------------------------------------------------------------
    // Kitty Graphics Protocol (Phase 4)
    // -------------------------------------------------------------------------

    /// Store the raw APC payload for the app layer to process.
    ///
    /// The app layer drains payloads via [`GridState::take_kitty_payloads`],
    /// parses them with [`crate::kitty::parse_kitty_command`], feeds them
    /// through a [`crate::kitty::KittyChunkAssembler`], decodes the image
    /// bytes, and uploads them to the GPU for rendering.
    fn kitty_graphics_command(&mut self, payload: &[u8]) {
        self.kitty_payloads.push(payload.to_vec());
    }

    fn shell_prompt_start(&mut self) {
        // OSC 133 ; A — no grid changes needed; marker for future use.
    }

    fn shell_command_start(&mut self) {
        // OSC 133 ; B — note that a command has begun.
        self.pending_command_start = true;
    }

    fn tool_list_query(&mut self) {
        self.tool_queries.push(());
    }

    fn tool_call(&mut self, name: String, args_json: String) {
        self.tool_calls.push((name, args_json));
    }

    fn shell_command_end(&mut self, exit_code: i32) {
        // OSC 133 ; D — record exit code in drain buffer.
        self.pending_command_start = false;
        self.shell_exit_codes.push(exit_code);
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

// ---------------------------------------------------------------------------
// Tests — Task 1 (Phase 4): StructuredContentAccumulator + Handler methods
// ---------------------------------------------------------------------------

#[cfg(test)]
mod phase4_task1_tests {
    use super::*;
    use arcterm_core::{Grid, GridSize};
    use std::collections::HashMap;

    fn make_gs() -> GridState {
        GridState::new(Grid::new(GridSize::new(24, 80)))
    }

    // --- ContentType enum ---

    #[test]
    fn content_type_variants_exist() {
        let _types = [
            ContentType::CodeBlock,
            ContentType::Diff,
            ContentType::Plan,
            ContentType::Markdown,
            ContentType::Json,
            ContentType::Error,
            ContentType::Progress,
            ContentType::Image,
        ];
    }

    // --- StructuredContentAccumulator construction ---

    #[test]
    fn accumulator_can_be_constructed() {
        let mut attrs = HashMap::new();
        attrs.insert("lang".to_string(), "rust".to_string());
        let acc = StructuredContentAccumulator::new(ContentType::CodeBlock, attrs.clone());
        assert!(matches!(acc.content_type, ContentType::CodeBlock));
        assert_eq!(acc.attrs.get("lang").map(|s| s.as_str()), Some("rust"));
        assert!(acc.buffer.is_empty());
    }

    #[test]
    fn accumulator_buffer_starts_empty() {
        let acc = StructuredContentAccumulator::new(ContentType::Json, HashMap::new());
        assert_eq!(acc.buffer, "");
    }

    // --- GridState accumulator field and completed_blocks ---

    #[test]
    fn grid_state_has_accumulator_field_none_by_default() {
        let gs = make_gs();
        assert!(gs.accumulator.is_none());
    }

    #[test]
    fn grid_state_has_completed_blocks_empty_by_default() {
        let gs = make_gs();
        assert!(gs.completed_blocks.is_empty());
    }

    // --- structured_content_start / structured_content_end via Handler ---

    #[test]
    fn structured_content_start_sets_accumulator() {
        let mut gs = make_gs();
        let mut attrs = HashMap::new();
        attrs.insert("lang".to_string(), "python".to_string());
        Handler::structured_content_start(&mut gs, ContentType::CodeBlock, attrs);
        assert!(gs.accumulator.is_some());
        let acc = gs.accumulator.as_ref().unwrap();
        assert!(matches!(acc.content_type, ContentType::CodeBlock));
        assert_eq!(acc.attrs.get("lang").map(|s| s.as_str()), Some("python"));
    }

    #[test]
    fn structured_content_end_moves_accumulator_to_completed() {
        let mut gs = make_gs();
        Handler::structured_content_start(&mut gs, ContentType::Markdown, HashMap::new());
        Handler::structured_content_end(&mut gs);
        assert!(gs.accumulator.is_none());
        assert_eq!(gs.completed_blocks.len(), 1);
        assert!(matches!(gs.completed_blocks[0].content_type, ContentType::Markdown));
    }

    #[test]
    fn structured_content_end_without_start_is_noop() {
        let mut gs = make_gs();
        // Must not panic or add a completed block.
        Handler::structured_content_end(&mut gs);
        assert!(gs.completed_blocks.is_empty());
    }

    // --- put_char during accumulation appends to buffer AND writes to grid ---

    #[test]
    fn put_char_during_accumulation_appends_to_buffer() {
        let mut gs = make_gs();
        Handler::structured_content_start(&mut gs, ContentType::CodeBlock, HashMap::new());
        Handler::put_char(&mut gs, 'H');
        Handler::put_char(&mut gs, 'i');
        assert_eq!(gs.accumulator.as_ref().unwrap().buffer, "Hi");
    }

    #[test]
    fn put_char_during_accumulation_also_writes_to_grid() {
        let mut gs = make_gs();
        Handler::structured_content_start(&mut gs, ContentType::CodeBlock, HashMap::new());
        Handler::put_char(&mut gs, 'X');
        // The char must appear in the grid at the cursor position (0,0).
        assert_eq!(gs.grid.cells[0][0].c, 'X');
    }

    #[test]
    fn put_char_without_accumulation_does_not_affect_accumulator() {
        let mut gs = make_gs();
        Handler::put_char(&mut gs, 'Z');
        assert!(gs.accumulator.is_none());
        assert_eq!(gs.grid.cells[0][0].c, 'Z');
    }

    // --- Multiple blocks ---

    #[test]
    fn multiple_blocks_accumulate_independently() {
        let mut gs = make_gs();

        // First block.
        Handler::structured_content_start(&mut gs, ContentType::CodeBlock, HashMap::new());
        Handler::put_char(&mut gs, 'A');
        Handler::structured_content_end(&mut gs);

        // Second block.
        Handler::structured_content_start(&mut gs, ContentType::Json, HashMap::new());
        Handler::put_char(&mut gs, 'B');
        Handler::structured_content_end(&mut gs);

        assert_eq!(gs.completed_blocks.len(), 2);
        assert_eq!(gs.completed_blocks[0].buffer, "A");
        assert_eq!(gs.completed_blocks[1].buffer, "B");
        assert!(matches!(gs.completed_blocks[0].content_type, ContentType::CodeBlock));
        assert!(matches!(gs.completed_blocks[1].content_type, ContentType::Json));
    }
}
