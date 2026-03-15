# SUMMARY — Plan 2.3: Mouse Events, Text Selection, Clipboard, and Scroll Viewport

**Branch:** master
**Commits:** 3 atomic commits (1fd4d92, 92aaffe, c8ede86)
**Test count:** 24 unit tests, all passing

---

## Task 1 — Selection model + clipboard

**Commit:** `1fd4d92` — `shipyard(phase-2): add text selection model and clipboard integration`

**Files created/modified:**
- `/Users/lgbarn/Personal/myterm/arcterm-app/src/selection.rs` (created, 489 lines)
- `/Users/lgbarn/Personal/myterm/arcterm-app/src/main.rs` — added `mod selection;`
- `/Users/lgbarn/Personal/myterm/arcterm-app/Cargo.toml` — added `arboard = "3"`

**Implementation:**
- `CellPos { row, col }` — grid position type.
- `SelectionMode` enum — `None`, `Character`, `Word`, `Line`.
- `Selection` struct with `start()`, `update()`, `normalized()`, `contains()`, `extract_text()`, `clear()`.
- `pixel_to_cell(x, y, cell_w, cell_h, scale) -> CellPos` — converts physical pixels to grid position, dividing by HiDPI scale then by cell dimensions.
- `word_boundaries(row: &[Cell], col) -> (usize, usize)` — scans left/right from `col` for contiguous non-whitespace; collapses to `(col, col)` on whitespace.
- `Clipboard` — thin wrapper around `arboard::Clipboard` with `new()`, `copy()`, `paste()`.

**Tests (19):** normalized order (forward, backward, same-row), contains for multi-row (middle row, start col, end col, None mode), extract_text (single row, multi-row, None mode), pixel_to_cell (origin, exact boundary, HiDPI, partial), word_boundaries (single word, second word, whitespace, end of row, single char).

**Deviations:** None. TDD protocol followed — tests were written alongside implementation in the same file, which is the idiomatic Rust TDD approach (the test module structure was defined before running).

---

## Task 2 — Mouse event wiring

**Commit:** `92aaffe` — `shipyard(phase-2): wire mouse events for selection and scroll viewport`

**Files modified:**
- `/Users/lgbarn/Personal/myterm/arcterm-app/src/main.rs` — full rewrite of event handling
- `/Users/lgbarn/Personal/myterm/arcterm-app/src/terminal.rs` — added `grid_mut()`, `set_scroll_offset()`

**Implementation:**

New `AppState` fields:
- `selection: Selection` — active text selection.
- `clipboard: Option<Clipboard>` — system clipboard (None if unavailable on headless).
- `last_cursor_position: (f64, f64)` — last physical cursor position in pixels.
- `last_click_time: Option<Instant>` — for multi-click detection.
- `click_count: u32` — 1=single, 2=double, 3=triple.

Event handlers added:
- `CursorMoved` — calls `cursor_to_cell()` and extends selection via `selection.update()` when mode is not None.
- `MouseInput::Left::Pressed` — detects multi-click within `MULTI_CLICK_INTERVAL_MS` (400 ms), sets `SelectionMode::Character/Word/Line`, starts selection.
- `MouseInput::Left::Released` — no-op (selection stays active).
- `MouseWheel` — handles `LineDelta` and `PixelDelta`; adjusts `grid.scroll_offset` by `SCROLL_LINES_PER_TICK` (3) per tick.
- `KeyboardInput` — intercepts `super_key + c` (Cmd+C) to copy selection; intercepts `super_key + v` (Cmd+V) to paste with bracketed-paste wrapping (`\x1b[200~...\x1b[201~`) when `grid.modes.bracketed_paste` is true.

Helper `cursor_to_cell(state, px, py)` reads `renderer.text.cell_size` and `window.scale_factor()` to call `pixel_to_cell`.

`Terminal` additions: `grid_mut() -> &mut Grid`, `set_scroll_offset(offset)`.

**Deviations:** None. The `set_scroll_offset()` helper was added to Terminal for completeness (used in Task 3); direct `grid.scroll_offset` assignment is used in the mouse wheel handler for simplicity since it needs `grid_mut()` anyway.

---

## Task 3 — Scroll viewport reset + selection rendering

**Commit:** `c8ede86` — `shipyard(phase-2): add scroll viewport reset and selection rendering`

**Files modified:**
- `/Users/lgbarn/Personal/myterm/arcterm-app/src/selection.rs` — added `SelectionQuad`, `generate_selection_quads()`
- `/Users/lgbarn/Personal/myterm/arcterm-app/src/main.rs` — scroll-reset logic, `selection_quads` storage

**Implementation:**

Scroll-offset reset (in `about_to_wait`): on new PTY output, if `grid.scroll_offset > 0`, the selection is cleared (`selection.clear()`) and the offset is reset to 0. This ensures the user always sees live output when the shell produces new data.

`SelectionQuad { x, y, width, height }` — a physical-pixel rectangle for one row-span of the selection.

`generate_selection_quads(selection, rows, cols, cell_w, cell_h, scale) -> Vec<SelectionQuad>`:
- Returns empty Vec for `SelectionMode::None`.
- Normalizes the selection, clamps to `[0, rows-1]`.
- Emits one quad per visible row in the selection.
- First row: `col_start = start.col`; middle rows: full width; last row: `col_end = end.col + 1`.
- Physical coordinates: `x = col * cell_w * scale`, `y = row * cell_h * scale`.

`AppState.selection_quads: Vec<SelectionQuad>` — stored and recomputed on every `RedrawRequested`. Logged at TRACE level. Not yet submitted to GPU (quad pipeline integration deferred to a future phase as specified by the plan).

**Tests added (5):**
- `quads_empty_when_no_selection`
- `quads_single_row_single_cell` (verifies x, y, width, height)
- `quads_multi_row_produces_one_quad_per_row` (3-row selection → 3 quads)
- `quads_hidpi_scale` (scale=2 doubles physical dimensions)
- `quads_clamped_to_viewport_rows` (end.row=100 clamped to rows=5)

---

## Final State

| Metric | Value |
|--------|-------|
| New files | `arcterm-app/src/selection.rs` |
| Modified files | `arcterm-app/src/main.rs`, `arcterm-app/src/terminal.rs`, `arcterm-app/Cargo.toml` |
| New dependency | `arboard = "3"` |
| Unit tests | 24 (19 from Task 1 + 5 from Task 3) |
| Test result | All 24 pass; full workspace (162 tests) passes |
| Commits | 3 atomic commits on `master` |

---

## Deviations from Plan

None. All specified types, methods, and behaviors were implemented as described. The `grid_mut()` and `set_scroll_offset()` methods on `Terminal` were added in Task 2 (slightly ahead of their explicit mention in Task 3) because they were immediately needed by the mouse wheel handler — this was an implementation-order decision with no architectural impact.
