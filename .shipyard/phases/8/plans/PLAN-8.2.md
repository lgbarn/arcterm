---
phase: config-overlays-polish-release
plan: "8.2"
wave: 1
dependencies: []
must_haves:
  - Leader+/ cross-pane search with regex
  - Match highlighting rendered as colored quads
  - n/N navigation between matches across panes
files_touched:
  - arcterm-app/src/search.rs
  - arcterm-app/src/main.rs
  - arcterm-core/src/grid.rs
tdd: true
---

# Plan 8.2 -- Cross-Pane Regex Search (Leader+/)

**Wave 1** | No dependencies | Parallel with Plan 8.1

## Goal

Implement the cross-pane search feature: Leader+/ opens a search overlay with a regex
input field, searches across all pane grids (visible rows + scrollback), highlights
matches as colored quads on the correct panes, and supports n/N navigation between
matches. This is a vertical slice from grid text extraction through search state
management and rendering.

---

<task id="1" files="arcterm-core/src/grid.rs, arcterm-app/src/search.rs" tdd="true">
  <action>
    1. In `arcterm-core/src/grid.rs`, add a public method `pub fn row_to_string(row: &[Cell]) -> String`
       that iterates cells in a row and builds a String from each cell's character. Also add
       `pub fn all_text_rows(&self) -> Vec<String>` that returns text for all scrollback rows
       (oldest first) followed by all visible rows, producing a contiguous text representation.
       Add tests: `row_to_string` on a row of ASCII cells produces the expected string;
       `all_text_rows` on a grid with scrollback returns scrollback rows before visible rows.

    2. Create `arcterm-app/src/search.rs` with `SearchOverlayState`:
       - Fields: `query: String`, `compiled: Option<regex::Regex>`,
         `matches: Vec<SearchMatch>`, `current_match: usize`, `error_msg: Option<String>`.
       - `SearchMatch` struct: `pane_id: PaneId`, `row_index: usize` (0 = oldest scrollback),
         `col_start: usize`, `col_end: usize`.
       - `fn new() -> Self` with empty state.
       - `fn update_query(&mut self, query: String)`: set `self.query`, attempt
         `regex::Regex::new(&query)`. On success set `self.compiled = Some(regex)` and clear
         `error_msg`. On failure set `self.compiled = None` and `self.error_msg = Some(err)`.
       - `fn execute_search(&mut self, panes: &[(PaneId, Vec<String>)])`: clear `matches`,
         iterate each pane's rows, call `compiled.find_iter(row_text)` and map byte offsets
         to column indices (handle UTF-8 by counting chars up to byte offset). Populate
         `self.matches` sorted by pane then row then col_start. Reset `current_match = 0`.
       - `fn next_match(&mut self)`: increment `current_match` modulo `matches.len()`.
       - `fn prev_match(&mut self)`: decrement `current_match` with wraparound.
       - `fn current(&self) -> Option<&SearchMatch>`: return current match if any.
       - `fn handle_key(&mut self, key: &Key) -> SearchAction`:
         `Enter` -> `SearchAction::Execute`, `Escape` -> `SearchAction::Close`,
         `Backspace` -> remove last char from query, printable char -> append to query,
         `n` (when query is compiled) -> `SearchAction::NextMatch`,
         `N` -> `SearchAction::PrevMatch`.
       - `SearchAction` enum: `UpdateQuery`, `Execute`, `Close`, `NextMatch`, `PrevMatch`, `Noop`.
       - Unit tests:
         a. `update_query` with valid regex compiles successfully, invalid regex sets error_msg
         b. `execute_search` finds matches at correct row/col positions across multiple mock panes
         c. `next_match`/`prev_match` wrap around correctly
         d. UTF-8 byte-to-column mapping: search for a pattern after a multi-byte char produces
            correct `col_start`/`col_end`
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-core -- row_to_string all_text_rows --no-fail-fast 2>&1 | tail -15 && cargo test --package arcterm-app -- search::tests --no-fail-fast 2>&1 | tail -20</verify>
  <done>Grid text extraction tests pass: `row_to_string` produces correct strings, `all_text_rows` includes scrollback before visible. Search tests pass: regex compilation, multi-pane match finding, n/N wraparound, UTF-8 column mapping.</done>
</task>

<task id="2" files="arcterm-app/src/search.rs, arcterm-app/src/main.rs" tdd="false">
  <action>
    Wire search overlay into AppState and the event loop:

    1. In `main.rs`, add `mod search;` declaration. Add
       `search_overlay: Option<search::SearchOverlayState>` to `AppState`, initialized to `None`.

    2. Wire `KeyAction::CrossPaneSearch` (added in Plan 8.1 Task 2) to create
       `SearchOverlayState::new()` and set `self.search_overlay = Some(state)`.

    3. In the keyboard input handler, when `self.search_overlay.is_some()`:
       - Route key events to `search_overlay.handle_key()`.
       - On `SearchAction::UpdateQuery`: call `search_overlay.update_query(query.clone())`.
       - On `SearchAction::Execute`: collect text rows from all panes by calling
         `terminal.grid().all_text_rows()` for each `(pane_id, terminal)` in `self.panes`,
         pass to `search_overlay.execute_search()`. Request redraw.
       - On `SearchAction::NextMatch` / `PrevMatch`: call `next_match()` / `prev_match()`.
         If current match is in a pane with a scroll offset, adjust that pane's
         `grid.scroll_offset` so the matched row is visible. Request redraw.
       - On `SearchAction::Close`: set `self.search_overlay = None`. Request redraw.

    4. In the render path (`about_to_wait`), when `self.search_overlay.is_some()`:
       - Build a bottom-anchored input bar (similar to palette): semi-transparent background
         quad spanning window width, ~40px tall. Render the query string as overlay text.
         Show match count (e.g., "3/17 matches") and error message if regex is invalid.
       - For each `SearchMatch` in `search_overlay.matches`, compute the pixel rect for the
         matched cell range within the correct pane's `rect` (using cell_size and the pane's
         physical pixel origin). Create an `OverlayQuad` with a yellow-tinted semi-transparent
         color `[1.0, 0.9, 0.0, 0.3]`.
       - For the `current_match`, use a brighter highlight `[1.0, 0.7, 0.0, 0.5]`.
       - Only render match quads for rows that are currently visible in each pane's viewport
         (check against `grid.scroll_offset` and visible row count).
       - Pass all quads through `render_multipane`'s `overlay_quads` parameter.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-app 2>&1 | tail -10</verify>
  <done>`cargo build --package arcterm-app` compiles without errors. Leader+/ opens search overlay. Typing a regex and pressing Enter searches all panes. Matches are highlighted as yellow quads on correct panes. n/N navigates between matches. Escape closes search.</done>
</task>

<task id="3" files="arcterm-app/src/search.rs" tdd="true">
  <action>
    Add edge case handling and integration refinements:

    1. Add debounced regex compilation: in `SearchOverlayState`, add a `last_query_change: Instant`
       field. In `update_query`, record the timestamp. Add a `fn should_auto_search(&self) -> bool`
       that returns true if >200ms have elapsed since `last_query_change` and `compiled.is_some()`.
       The event loop calls this in `about_to_wait` to trigger automatic search without requiring
       Enter.

    2. Add `fn match_quads_for_pane(&self, pane_id: PaneId, pane_rect: [f32; 4], cell_w: f32, cell_h: f32, scroll_offset: usize, visible_rows: usize) -> Vec<(OverlayQuad, bool)>`
       that returns `(quad, is_current)` tuples for all matches visible in the pane's viewport.
       This encapsulates the pixel-rect math and keeps main.rs rendering code clean.

    3. Handle search across scrollback: when `current_match` points to a row in scrollback,
       auto-scroll the target pane so the match row is visible. Add
       `fn scroll_offset_for_match(match_row: usize, total_scrollback: usize, visible_rows: usize) -> usize`
       that computes the correct `scroll_offset` to center the match row in the viewport.

    4. Unit tests:
       - `should_auto_search` returns false immediately after query change, true after 200ms
       - `match_quads_for_pane` returns empty vec when no matches in pane
       - `match_quads_for_pane` returns correct pixel rects for matches in visible viewport
       - `scroll_offset_for_match` centers the match row when deep in scrollback
       - Empty query produces no matches and no error
       - Query with no results: `matches` is empty, `current()` returns `None`
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-app -- search::tests --no-fail-fast 2>&1 | tail -25</verify>
  <done>All search tests pass including debounce timing, quad generation for visible viewport, scrollback offset calculation, and empty/no-result edge cases. Search overlay is feature-complete.</done>
</task>
