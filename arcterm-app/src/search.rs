//! Cross-pane regex search overlay state machine.
//!
//! `SearchOverlayState` is opened by Leader+/ and searches all pane grids
//! (visible rows + scrollback) for a user-supplied regex.  Matches are
//! rendered as coloured quads; n/N navigate between them.

use std::time::{Duration, Instant};

use crate::layout::PaneId;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single regex match found in a pane's text content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    /// The pane in which this match was found.
    pub pane_id: PaneId,
    /// Row index in the full text representation (0 = oldest scrollback row).
    pub row_index: usize,
    /// Column index (character offset) of the start of the match.
    pub col_start: usize,
    /// Column index (character offset) of the end of the match (exclusive).
    pub col_end: usize,
}

/// A coloured quad used to highlight a search match in the rendered output.
#[derive(Debug, Clone, Copy)]
pub struct OverlayQuad {
    /// Bounding rectangle in physical pixels: [x, y, width, height].
    pub rect: [f32; 4],
    /// RGBA colour in [0, 1].
    pub color: [f32; 4],
}

/// Actions produced by [`SearchOverlayState::handle_key`].
#[derive(Debug, PartialEq)]
pub enum SearchAction {
    /// The query string was updated (typing or backspace).
    UpdateQuery,
    /// The user pressed Enter — execute the search now.
    Execute,
    /// The user pressed Escape — close the overlay.
    Close,
    /// Navigate to the next match.
    NextMatch,
    /// Navigate to the previous match.
    PrevMatch,
    /// Key was consumed but produced no state change.
    Noop,
}

// ---------------------------------------------------------------------------
// SearchOverlayState
// ---------------------------------------------------------------------------

/// Runtime state of the cross-pane search overlay.
pub struct SearchOverlayState {
    /// Current query string typed by the user.
    pub query: String,
    /// Compiled regex, or `None` when the query is empty or invalid.
    pub compiled: Option<regex::Regex>,
    /// All matches found by the most recent `execute_search` call.
    pub matches: Vec<SearchMatch>,
    /// Index into `matches` pointing at the currently focused match.
    pub current_match: usize,
    /// Human-readable error message when regex compilation fails.
    pub error_msg: Option<String>,
    /// Timestamp of the most recent query change (for debounced auto-search).
    pub last_query_change: Instant,
}

impl SearchOverlayState {
    /// Create a new, empty search overlay.
    pub fn new() -> Self {
        Self {
            query: String::new(),
            compiled: None,
            matches: Vec::new(),
            current_match: 0,
            error_msg: None,
            last_query_change: Instant::now(),
        }
    }

    // -----------------------------------------------------------------------
    // Query management
    // -----------------------------------------------------------------------

    /// Update the query string and attempt to compile it as a regex.
    ///
    /// On success `compiled` is set and `error_msg` is cleared.
    /// On failure `compiled` is set to `None` and `error_msg` is set.
    pub fn update_query(&mut self, query: String) {
        self.last_query_change = Instant::now();
        self.query = query;
        if self.query.is_empty() {
            self.compiled = None;
            self.error_msg = None;
            return;
        }
        match regex::Regex::new(&self.query) {
            Ok(re) => {
                self.compiled = Some(re);
                self.error_msg = None;
            }
            Err(e) => {
                self.compiled = None;
                self.error_msg = Some(e.to_string());
            }
        }
    }

    // -----------------------------------------------------------------------
    // Search execution
    // -----------------------------------------------------------------------

    /// Search all panes for the compiled regex.
    ///
    /// `panes` is a slice of `(pane_id, text_rows)` where `text_rows[0]` is
    /// the oldest scrollback row and `text_rows[last]` is the bottom visible row.
    ///
    /// Populates `self.matches` sorted by pane, then row, then col_start.
    /// Resets `current_match` to 0.
    pub fn execute_search(&mut self, panes: &[(PaneId, Vec<String>)]) {
        self.matches.clear();
        self.current_match = 0;

        let re = match &self.compiled {
            Some(r) => r,
            None => return,
        };

        for (pane_id, rows) in panes.iter() {
            for (row_index, row_text) in rows.iter().enumerate() {
                for m in re.find_iter(row_text) {
                    // Convert byte offsets to character (column) indices.
                    let col_start = byte_offset_to_char_index(row_text, m.start());
                    let col_end = byte_offset_to_char_index(row_text, m.end());
                    self.matches.push(SearchMatch {
                        pane_id: *pane_id,
                        row_index,
                        col_start,
                        col_end,
                    });
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Navigation
    // -----------------------------------------------------------------------

    /// Advance to the next match, wrapping around.
    pub fn next_match(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.current_match = (self.current_match + 1) % self.matches.len();
    }

    /// Go back to the previous match, wrapping around.
    pub fn prev_match(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        if self.current_match == 0 {
            self.current_match = self.matches.len() - 1;
        } else {
            self.current_match -= 1;
        }
    }

    /// Return the current match, if any.
    pub fn current(&self) -> Option<&SearchMatch> {
        self.matches.get(self.current_match)
    }

    // -----------------------------------------------------------------------
    // Keyboard handling
    // -----------------------------------------------------------------------

    /// Process a logical key and return the action to take.
    ///
    /// The caller should inspect the returned [`SearchAction`] and act on it:
    /// - `UpdateQuery` — call `update_query` with the new query string.
    /// - `Execute` — call `execute_search` then request a redraw.
    /// - `Close` — set `search_overlay = None`.
    /// - `NextMatch` / `PrevMatch` — call `next_match()` / `prev_match()`.
    /// - `Noop` — do nothing.
    pub fn handle_key(&mut self, key: &winit::keyboard::Key) -> SearchAction {
        use winit::keyboard::{Key, NamedKey};
        match key {
            Key::Named(NamedKey::Escape) => SearchAction::Close,
            Key::Named(NamedKey::Enter) => SearchAction::Execute,
            Key::Named(NamedKey::Backspace) => {
                self.query.pop();
                let q = self.query.clone();
                self.update_query(q);
                SearchAction::UpdateQuery
            }
            Key::Character(s) => {
                let ch = s.as_str();
                // When the overlay is in search mode (compiled regex), n/N navigate.
                if self.compiled.is_some() && ch == "n" {
                    return SearchAction::NextMatch;
                }
                if self.compiled.is_some() && ch == "N" {
                    return SearchAction::PrevMatch;
                }
                // Otherwise append to query.
                let new_query = format!("{}{}", self.query, ch);
                self.update_query(new_query);
                SearchAction::UpdateQuery
            }
            _ => SearchAction::Noop,
        }
    }

    // -----------------------------------------------------------------------
    // Debounced auto-search
    // -----------------------------------------------------------------------

    /// Returns `true` if enough time has elapsed since the last query change
    /// and the regex is valid — the event loop should trigger an automatic search.
    pub fn should_auto_search(&self) -> bool {
        self.compiled.is_some()
            && self.last_query_change.elapsed() >= Duration::from_millis(200)
    }

    // -----------------------------------------------------------------------
    // Rendering helpers
    // -----------------------------------------------------------------------

    /// Compute pixel quads for all matches visible in the given pane's viewport.
    ///
    /// Returns a `Vec<(OverlayQuad, bool)>` where the `bool` is `true` for the
    /// currently-focused match.
    ///
    /// `pane_rect` — [x, y, width, height] in physical pixels.
    /// `cell_w` / `cell_h` — cell dimensions in physical pixels.
    /// `scroll_offset` — how many rows above the live screen the viewport is scrolled.
    /// `visible_rows` — total number of rows in the viewport.
    /// `total_rows` — total number of rows in `all_text_rows()` for this pane
    ///   (scrollback_len + grid rows).
    // All 8 parameters are distinct physical quantities; a struct wrapper
    // would add boilerplate without improving clarity at call sites.
    #[allow(clippy::too_many_arguments)]
    pub fn match_quads_for_pane(
        &self,
        pane_id: PaneId,
        pane_rect: [f32; 4],
        cell_w: f32,
        cell_h: f32,
        scroll_offset: usize,
        visible_rows: usize,
        total_rows: usize,
    ) -> Vec<(OverlayQuad, bool)> {
        let mut out = Vec::new();

        // The viewport shows rows [viewport_start, viewport_start + visible_rows).
        // In all_text_rows coordinates:
        //   - row 0 is the oldest scrollback row
        //   - rows [total_rows - visible_rows - scroll_offset .. total_rows - scroll_offset)
        //     map to the viewport when scrolled.
        // When scroll_offset == 0:
        //   viewport shows [total_rows - visible_rows, total_rows).
        // When scroll_offset > 0 (scrolled up):
        //   viewport shows [total_rows - visible_rows - scroll_offset,
        //                   total_rows - scroll_offset).
        let viewport_start = total_rows
            .saturating_sub(visible_rows)
            .saturating_sub(scroll_offset);
        let viewport_end = total_rows.saturating_sub(scroll_offset);

        for (match_idx, m) in self.matches.iter().enumerate() {
            if m.pane_id != pane_id {
                continue;
            }
            if m.row_index < viewport_start || m.row_index >= viewport_end {
                continue;
            }

            // Row within viewport (0-indexed from viewport top).
            let vp_row = (m.row_index - viewport_start) as f32;
            let x = pane_rect[0] + m.col_start as f32 * cell_w;
            let y = pane_rect[1] + vp_row * cell_h;
            let w = (m.col_end - m.col_start) as f32 * cell_w;
            let h = cell_h;

            let is_current = match_idx == self.current_match;
            let color = if is_current {
                [1.0, 0.7, 0.0, 0.5]
            } else {
                [1.0, 0.9, 0.0, 0.3]
            };

            out.push((
                OverlayQuad { rect: [x, y, w, h], color },
                is_current,
            ));
        }

        out
    }

    /// Compute the `scroll_offset` that centres `match_row` in the viewport.
    ///
    /// `match_row` — row index in `all_text_rows()` space.
    /// `total_rows` — total rows in `all_text_rows()`.
    /// `visible_rows` — number of visible rows in the pane viewport.
    ///
    /// Returns the scroll_offset value to set on the pane's grid.
    pub fn scroll_offset_for_match(
        match_row: usize,
        total_rows: usize,
        visible_rows: usize,
    ) -> usize {
        // The live view (scroll_offset == 0) shows [total_rows - visible_rows, total_rows).
        // To centre match_row:
        //   desired_viewport_start = match_row - visible_rows / 2
        //   viewport_start = total_rows - visible_rows - scroll_offset
        //   => scroll_offset = total_rows - visible_rows - desired_viewport_start
        let half = visible_rows / 2;
        let desired_start = match_row.saturating_sub(half);
        // scroll_offset cannot push past the available scrollback.
        let live_start = total_rows.saturating_sub(visible_rows);
        live_start.saturating_sub(desired_start)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a byte offset in a UTF-8 string to a character (column) index.
fn byte_offset_to_char_index(s: &str, byte_offset: usize) -> usize {
    s[..byte_offset].chars().count()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::PaneId;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_pane_id() -> PaneId {
        PaneId::next()
    }

    // -----------------------------------------------------------------------
    // a. update_query: valid regex compiles, invalid sets error_msg
    // -----------------------------------------------------------------------

    #[test]
    fn update_query_valid_regex_compiles() {
        let mut s = SearchOverlayState::new();
        s.update_query("foo.*bar".to_string());
        assert!(s.compiled.is_some(), "valid regex must compile");
        assert!(s.error_msg.is_none(), "no error message for valid regex");
        assert_eq!(s.query, "foo.*bar");
    }

    #[test]
    fn update_query_invalid_regex_sets_error_msg() {
        let mut s = SearchOverlayState::new();
        s.update_query("(unclosed".to_string());
        assert!(s.compiled.is_none(), "invalid regex must not compile");
        assert!(s.error_msg.is_some(), "error_msg must be set for invalid regex");
    }

    #[test]
    fn update_query_empty_clears_compiled_and_error() {
        let mut s = SearchOverlayState::new();
        s.update_query("foo".to_string());
        assert!(s.compiled.is_some());
        s.update_query(String::new());
        assert!(s.compiled.is_none(), "empty query must clear compiled");
        assert!(s.error_msg.is_none(), "empty query must clear error_msg");
        assert!(s.matches.is_empty(), "no matches for empty query");
    }

    // -----------------------------------------------------------------------
    // b. execute_search: finds matches at correct row/col
    // -----------------------------------------------------------------------

    #[test]
    fn execute_search_finds_matches_in_multiple_panes() {
        let mut s = SearchOverlayState::new();
        s.update_query("foo".to_string());

        let id1 = make_pane_id();
        let id2 = make_pane_id();
        let panes: Vec<(PaneId, Vec<String>)> = vec![
            (id1, vec!["no match here".to_string(), "found foo here".to_string()]),
            (id2, vec!["foo at start".to_string(), "nothing".to_string()]),
        ];
        s.execute_search(&panes);

        assert_eq!(s.matches.len(), 2, "two 'foo' occurrences total");
        // Match in pane1, row 1, column 6
        let m0 = &s.matches[0];
        assert_eq!(m0.pane_id, id1);
        assert_eq!(m0.row_index, 1);
        assert_eq!(m0.col_start, 6);
        assert_eq!(m0.col_end, 9);
        // Match in pane2, row 0, column 0
        let m1 = &s.matches[1];
        assert_eq!(m1.pane_id, id2);
        assert_eq!(m1.row_index, 0);
        assert_eq!(m1.col_start, 0);
        assert_eq!(m1.col_end, 3);
    }

    #[test]
    fn execute_search_no_compiled_produces_no_matches() {
        let mut s = SearchOverlayState::new();
        // Don't set a query — compiled is None.
        let id = make_pane_id();
        s.execute_search(&[(id, vec!["some text".to_string()])]);
        assert!(s.matches.is_empty(), "no compiled regex means no matches");
    }

    // -----------------------------------------------------------------------
    // c. next_match / prev_match wrap around correctly
    // -----------------------------------------------------------------------

    fn make_state_with_n_matches(n: usize) -> SearchOverlayState {
        let mut s = SearchOverlayState::new();
        s.update_query("x".to_string());
        let id = make_pane_id();
        let rows: Vec<String> = (0..n).map(|_| "x".to_string()).collect();
        s.execute_search(&[(id, rows)]);
        s
    }

    #[test]
    fn next_match_advances_index() {
        let mut s = make_state_with_n_matches(3);
        assert_eq!(s.current_match, 0);
        s.next_match();
        assert_eq!(s.current_match, 1);
        s.next_match();
        assert_eq!(s.current_match, 2);
    }

    #[test]
    fn next_match_wraps_from_last_to_first() {
        let mut s = make_state_with_n_matches(3);
        s.current_match = 2;
        s.next_match();
        assert_eq!(s.current_match, 0, "next_match must wrap from last to first");
    }

    #[test]
    fn prev_match_decrements_index() {
        let mut s = make_state_with_n_matches(3);
        s.current_match = 2;
        s.prev_match();
        assert_eq!(s.current_match, 1);
    }

    #[test]
    fn prev_match_wraps_from_first_to_last() {
        let mut s = make_state_with_n_matches(3);
        s.current_match = 0;
        s.prev_match();
        assert_eq!(s.current_match, 2, "prev_match must wrap from first to last");
    }

    #[test]
    fn next_prev_match_no_op_on_empty() {
        let mut s = SearchOverlayState::new();
        s.next_match(); // must not panic
        s.prev_match(); // must not panic
        assert!(s.current().is_none());
    }

    // -----------------------------------------------------------------------
    // d. UTF-8 byte-to-column mapping
    // -----------------------------------------------------------------------

    #[test]
    fn execute_search_utf8_column_mapping() {
        let mut s = SearchOverlayState::new();
        // Pattern after a 3-byte UTF-8 character (€ = U+20AC, 3 bytes).
        s.update_query("world".to_string());
        let id = make_pane_id();
        // "€ world" — '€' is 3 bytes but 1 char, space is 1 byte/char.
        // "world" starts at char index 2.
        let row = "\u{20AC} world".to_string();
        s.execute_search(&[(id, vec![row])]);
        assert_eq!(s.matches.len(), 1);
        let m = &s.matches[0];
        assert_eq!(m.col_start, 2, "col_start must be char index 2, not byte index 4");
        assert_eq!(m.col_end, 7, "col_end must be char index 7");
    }

    // -----------------------------------------------------------------------
    // should_auto_search debounce
    // -----------------------------------------------------------------------

    #[test]
    fn should_auto_search_false_immediately_after_query_change() {
        let mut s = SearchOverlayState::new();
        s.update_query("foo".to_string());
        // Immediately after update the debounce has not elapsed.
        assert!(
            !s.should_auto_search(),
            "should_auto_search must be false immediately after query change"
        );
    }

    #[test]
    fn should_auto_search_true_after_debounce_elapsed() {
        let mut s = SearchOverlayState::new();
        s.update_query("foo".to_string());
        // Backdate last_query_change to simulate time passing.
        s.last_query_change =
            Instant::now() - Duration::from_millis(201);
        assert!(
            s.should_auto_search(),
            "should_auto_search must be true after 200ms"
        );
    }

    #[test]
    fn should_auto_search_false_when_no_compiled_regex() {
        let mut s = SearchOverlayState::new();
        // No query — compiled is None.
        s.last_query_change = Instant::now() - Duration::from_millis(500);
        assert!(
            !s.should_auto_search(),
            "should_auto_search must be false when compiled is None"
        );
    }

    // -----------------------------------------------------------------------
    // match_quads_for_pane
    // -----------------------------------------------------------------------

    #[test]
    fn match_quads_for_pane_empty_when_no_matches_in_pane() {
        let mut s = SearchOverlayState::new();
        s.update_query("foo".to_string());
        let id1 = make_pane_id();
        let id2 = make_pane_id();
        let panes = vec![(id1, vec!["foo".to_string()])];
        s.execute_search(&panes);
        // Asking for id2 — no matches there.
        let quads = s.match_quads_for_pane(
            id2, [0.0, 0.0, 800.0, 600.0], 8.0, 16.0, 0, 24, 24,
        );
        assert!(quads.is_empty(), "must return empty vec when no matches in pane");
    }

    #[test]
    fn match_quads_for_pane_correct_pixel_rects() {
        let mut s = SearchOverlayState::new();
        s.update_query("foo".to_string());
        let id = make_pane_id();
        // 5 total rows: 0 scrollback + 5 visible (scroll_offset = 0)
        let rows = vec![
            "     ".to_string(), // row 0
            "     ".to_string(), // row 1
            "foo  ".to_string(), // row 2 — match at col 0-3
            "     ".to_string(), // row 3
            "     ".to_string(), // row 4
        ];
        s.execute_search(&[(id, rows)]);
        let pane_rect = [10.0, 20.0, 400.0, 80.0];
        let cell_w = 8.0;
        let cell_h = 16.0;
        let scroll_offset = 0;
        let visible_rows = 5;
        let total_rows = 5;
        let quads = s.match_quads_for_pane(
            id, pane_rect, cell_w, cell_h, scroll_offset, visible_rows, total_rows,
        );
        assert_eq!(quads.len(), 1);
        let (q, is_current) = quads[0];
        assert!(is_current, "first match is the current match");
        // Row 2 in viewport => y = pane_y + 2 * cell_h = 20 + 32 = 52
        assert_eq!(q.rect[0], 10.0, "x must be pane_x + col_start*cell_w");
        assert!((q.rect[1] - 52.0).abs() < 0.001, "y must be pane_y + row*cell_h");
        assert!((q.rect[2] - 24.0).abs() < 0.001, "width must be (col_end-col_start)*cell_w");
        assert!((q.rect[3] - 16.0).abs() < 0.001, "height must be cell_h");
    }

    #[test]
    fn match_quads_for_pane_skips_rows_outside_viewport() {
        let mut s = SearchOverlayState::new();
        s.update_query("x".to_string());
        let id = make_pane_id();
        // 10 total rows; visible_rows = 5, scroll_offset = 0
        // Viewport shows rows 5-9 (in all_text_rows space).
        let mut rows: Vec<String> = vec!["     ".to_string(); 10];
        rows[2] = "x    ".to_string(); // row 2 — outside viewport
        rows[7] = "x    ".to_string(); // row 7 — inside viewport
        s.execute_search(&[(id, rows)]);
        let quads = s.match_quads_for_pane(
            id, [0.0, 0.0, 400.0, 80.0], 8.0, 16.0, 0, 5, 10,
        );
        assert_eq!(quads.len(), 1, "only match at row 7 is in the viewport");
    }

    // -----------------------------------------------------------------------
    // scroll_offset_for_match
    // -----------------------------------------------------------------------

    #[test]
    fn scroll_offset_for_match_centers_deep_scrollback_row() {
        // 100 total rows, 24 visible, match at row 10 (deep in scrollback).
        // live_start = 100 - 24 = 76
        // desired_start = 10 - 12 = 0 (clamped to 0 via saturating_sub)
        // scroll_offset = 76 - 0 = 76
        let offset = SearchOverlayState::scroll_offset_for_match(10, 100, 24);
        assert_eq!(offset, 76, "deep scrollback match must produce large scroll_offset");
    }

    #[test]
    fn scroll_offset_for_match_visible_row_returns_zero() {
        // 50 total rows, 24 visible. Visible rows are [26, 50).
        // Match at row 40 (in visible area) — no scroll needed.
        let offset = SearchOverlayState::scroll_offset_for_match(40, 50, 24);
        assert_eq!(offset, 0, "match in visible area must return scroll_offset = 0");
    }
}
