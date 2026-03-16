# Phase 10 Research — Application Input and UX Fixes

**Prepared for:** Phase 10 planning
**Crates in scope:** `arcterm-app`, `arcterm-render`
**Primary files:** `arcterm-app/src/main.rs`, `arcterm-app/src/input.rs`, `arcterm-app/src/terminal.rs`, `arcterm-render/src/text.rs`, `arcterm-render/src/renderer.rs`

---

## Summary of Findings

The most important finding for the planner: **four of the five ISSUE-XXX items are already implemented** in the current codebase. The original ISSUES.md descriptions document what _was_ absent at review time; development continued and all five were addressed before this phase was formally planned. The remaining real work is:

1. **MIGRATION (blocking, 8 compile errors):** `scroll_offset` was made private in Phase 9. `arcterm-app` does not compile until these accesses are updated to the accessor API.
2. **ISSUE-006 (partial, needs clarification):** The cursor is already rendered as a solid quad block via the `QuadRenderer` path. The `shape_row_into_buffer` function in `text.rs` does not substitute a block glyph for blank cells, but this may be fine because the cursor block is drawn at the quad layer, not the text layer. Verification is required.

---

## Compile Status

```
cargo check -p arcterm-app   →   8 errors (all E0616: field `scroll_offset` is private)
cargo check -p arcterm-core  →   clean
cargo check -p arcterm-render →  clean
```

`arcterm-app` will not compile at all until the MIGRATION task is resolved. All Phase 10 tests depend on compilation succeeding.

---

## MIGRATION: scroll_offset API — Detailed Investigation

### Background

Phase 9 commit `1996db9` changed `pub scroll_offset: usize` to `scroll_offset: usize` (private) in `arcterm-core/src/grid.rs` and added:

```rust
// arcterm-core/src/grid.rs:267-273
pub fn set_scroll_offset(&mut self, offset: usize) {
    self.scroll_offset = offset.min(self.scrollback.len());
}
pub fn scroll_offset(&self) -> usize {
    self.scroll_offset
}
```

The Phase 9 plan documented that `arcterm-app` would break and deferred the fix to Phase 10.

### All 8 Compile Error Locations

**File 1: `arcterm-app/src/terminal.rs:199`**

```rust
// Line 197-199 — Terminal::set_scroll_offset() wrapper method
pub fn set_scroll_offset(&mut self, offset: usize) {
    let max = self.grid_state.grid.scrollback.len();
    self.grid_state.grid.scroll_offset = offset.min(max);  // ERROR
}
```

This method is a thin wrapper around the private field. Now that `Grid::set_scroll_offset()` exists and performs the same clamping, this method body should delegate to `self.grid_state.grid.set_scroll_offset(offset)`. The `scrollback.len()` read can be removed since the `Grid` method handles the clamp internally.

**File 2: `arcterm-app/src/main.rs:1692` — PTY got_data path: read**

```rust
// Line 1691-1695 — about_to_wait(): scroll-to-live on PTY data arrival
let grid = terminal.grid_mut();
if grid.scroll_offset > 0 {        // ERROR (read)
    state.selection.clear();
    grid.scroll_offset = 0;        // ERROR (write)
}
```

Pattern: read the current offset, then reset to 0. Replace with:
```rust
if grid.scroll_offset() > 0 {
    state.selection.clear();
    grid.set_scroll_offset(0);
}
```

**File 2: `arcterm-app/src/main.rs:1981,1984` — MouseWheel handler**

```rust
// Lines 1979-1984 — MouseWheel scroll
let grid = terminal.grid_mut();
let max_offset = grid.scrollback.len();         // still public field — OK
let current = grid.scroll_offset as i32;        // ERROR (read)
let new_offset = (current - lines * SCROLL_LINES_PER_TICK as i32)
    .clamp(0, max_offset as i32) as usize;
grid.scroll_offset = new_offset;               // ERROR (write)
```

Note: `grid.scrollback` is still a public `VecDeque` field. The clamping arithmetic here can be simplified by calling `grid.set_scroll_offset(new_offset)` directly (the setter performs its own clamp), making the `max_offset` variable unnecessary. Replace with:
```rust
let current = grid.scroll_offset() as i32;
let new_offset = (current - lines * SCROLL_LINES_PER_TICK as i32)
    .max(0) as usize;
grid.set_scroll_offset(new_offset);
```

**File 2: `arcterm-app/src/main.rs:2343` — Search overlay quad computation: read**

```rust
// Lines 2340-2349 — search overlay match highlighting
let scroll_offset = grid.scroll_offset;   // ERROR (read)
let quads = so.match_quads_for_pane(
    pane_id,
    [rect.x, rect.y, rect.width, rect.height],
    cell_w,
    cell_h,
    scroll_offset,
    visible_rows,
    total_rows,
);
```

Replace `grid.scroll_offset` with `grid.scroll_offset()`.

**File 2: `arcterm-app/src/main.rs:2651-2652` — Search: NextMatch scroll**

```rust
// Lines 2647-2656 — SearchAction::NextMatch
let total = terminal.grid().all_text_rows().len();
let visible = terminal.grid().size.rows;
terminal.grid_mut().scroll_offset =         // ERROR (write)
    search::SearchOverlayState::scroll_offset_for_match(
        m.row_index,
        total,
        visible,
    );
```

Replace `.scroll_offset =` with `.set_scroll_offset(...)`.

**File 2: `arcterm-app/src/main.rs:2671-2672` — Search: PrevMatch scroll**

```rust
// Lines 2667-2676 — SearchAction::PrevMatch (identical pattern)
terminal.grid_mut().scroll_offset =         // ERROR (write)
    search::SearchOverlayState::scroll_offset_for_match(
        m.row_index,
        total,
        visible,
    );
```

Same fix as NextMatch.

### Summary Table: MIGRATION Changes

| Location | File | Line | Type | Fix |
|---|---|---|---|---|
| `terminal.rs:199` | `arcterm-app/src/terminal.rs` | 199 | write | delegate to `grid.set_scroll_offset(offset)` |
| `about_to_wait` got_data | `arcterm-app/src/main.rs` | 1692 | read | `.scroll_offset()` |
| `about_to_wait` got_data | `arcterm-app/src/main.rs` | 1694 | write | `.set_scroll_offset(0)` |
| `MouseWheel` handler | `arcterm-app/src/main.rs` | 1981 | read | `.scroll_offset()` |
| `MouseWheel` handler | `arcterm-app/src/main.rs` | 1984 | write | `.set_scroll_offset(new_offset)` |
| Search overlay quads | `arcterm-app/src/main.rs` | 2343 | read | `.scroll_offset()` |
| Search NextMatch | `arcterm-app/src/main.rs` | 2651 | write | `.set_scroll_offset(...)` |
| Search PrevMatch | `arcterm-app/src/main.rs` | 2671 | write | `.set_scroll_offset(...)` |

**Note on `overlay.rs`:** Lines 2550, 2573, 2596, 2606 in `main.rs` access `review.scroll_offset` where `review` is `overlay::OverlayReviewState`. This is a _different_ `scroll_offset` field on a different struct — it is public and unaffected by the Phase 9 change.

**Note on `grid.scrollback`:** The `scrollback` field of `Grid` is still public. The MouseWheel handler reads `grid.scrollback.len()` for clamping arithmetic. This access is legal and requires no change; the `set_scroll_offset()` setter handles its own clamping, so the explicit `max_offset` variable can optionally be removed as a cleanup.

---

## ISSUE-002: Missing request_redraw() After Keyboard Input

### Issue Description (from ISSUES.md)
File: `arcterm-app/src/main.rs:205` — The `KeyboardInput` handler calls `terminal.write_input` but does not follow with `window.request_redraw()`.

### Current State: **Already Fixed**

The `KeyboardInput` handler at line 2403 has `request_redraw()` in place. The path where raw PTY bytes are written at line 2754 reads:

```rust
// arcterm-app/src/main.rs:2753-2755
} else if let Some(terminal) = state.panes.get_mut(&focused_id) {
    terminal.write_input(&bytes);
    state.window.request_redraw();   // present
}
```

The surrounding code also has `request_redraw()` calls at lines 2449 (clipboard paste), 2471, 2486, 2489, 2502, 2510, 2513, and many others throughout the keyboard handler. The original line 205 referenced by the issue corresponds to a substantially different code layout in the current file; the file has grown significantly since the review.

**No work required for ISSUE-002.**

---

## ISSUE-003: Ctrl+\ and Ctrl+] Not Handled

### Issue Description (from ISSUES.md)
File: `arcterm-app/src/input.rs:22-28` — Ctrl+`\` and Ctrl+`]` not mapped.

### Current State: **Already Fixed**

The `translate_key_event` function in `arcterm-app/src/input.rs` contains:

```rust
// arcterm-app/src/input.rs:33-40
// Ctrl+\ → FS (0x1c, SIGQUIT in terminals)
if lower == '\\' {
    return Some(vec![0x1c]);
}
// Ctrl+] → GS (0x1d, telnet escape)
if lower == ']' {
    return Some(vec![0x1d]);
}
```

Both mappings are present and correctly placed within the `Key::Character(s) if ctrl` arm of the match, after the alphabetic check and the `'['` → ESC check (line 30).

**Test coverage:** The existing test module at the bottom of `input.rs` covers arrow keys, home/end, page keys, and function keys, but does not include tests for `ctrl_backslash` or `ctrl_bracket_right`. New tests are needed to prevent regression.

**No code fix required for ISSUE-003. One regression test is needed per the Phase 10 success criteria.**

---

## ISSUE-004: PTY Creation .expect() Panics

### Issue Description (from ISSUES.md)
File: `arcterm-app/src/main.rs:88` — `Terminal::new(size).expect("failed to create PTY session")` panics on failure.

### Current State: **Already Fixed**

The `spawn_default_pane` function (line 344) uses `unwrap_or_else`:

```rust
// arcterm-app/src/main.rs:346-350
let (mut terminal, pty_rx) =
    Terminal::new(initial_size, cfg.shell.clone(), None).unwrap_or_else(|e| {
        log::error!("Failed to create PTY session: {e}");
        std::process::exit(1);
    });
```

The `spawn_pane_with_cwd` method (line 828) uses a full `match` with error logging:

```rust
// arcterm-app/src/main.rs:830-843
match Terminal::new(size, self.config.shell.clone(), cwd) {
    Ok((mut terminal, rx)) => { ... }
    Err(e) => {
        log::error!("Failed to create PTY for pane {:?}: {e}", id);
    }
}
```

The workspace restore path at line 910 also uses `match` with identical error handling.

**The `window.expect("failed to create window")` at line 1000 is a different call — it panics on GPU/window creation failure and is addressed separately as M-5 (Phase 11), not ISSUE-004.**

**No work required for ISSUE-004.**

---

## ISSUE-005: No Shell Exit Indicator

### Issue Description (from ISSUES.md)
File: `arcterm-app/src/main.rs:121-126` — PTY channel close logs but does not show in-window indicator.

### Current State: **Already Fixed**

The `shell_exited: bool` field exists on `AppState` (line 554). The PTY drain loop sets it:

```rust
// arcterm-app/src/main.rs:1681-1684
if state.pty_channels.is_empty() {
    log::info!("All PTY channels closed — shell has exited");
    state.shell_exited = true;
    state.window.request_redraw();
}
```

The `RedrawRequested` handler at line 2089 renders a banner when `shell_exited` is true:

```rust
// arcterm-app/src/main.rs:2089-2128
if state.shell_exited {
    // Show exit banner on the focused pane (or first available).
    let target_id = focused;
    if let Some(terminal) = state.panes.get(&target_id) {
        let mut display = terminal.grid().clone();
        let last_row = display.size.rows.saturating_sub(1);
        let msg = "[ Shell exited — press any key to close ]";
        let banner_attrs = CellAttrs {
            fg: Color::Indexed(11),  // bright yellow
            bg: Color::Indexed(0),   // black
            bold: true,
            ..CellAttrs::default()
        };
        // ... writes the banner into the last row of the cloned grid
        state.renderer.render_frame(&display, scale);
        return;
    }
}
```

The `about_to_wait` handler at line 1236 also short-circuits processing when `shell_exited` is true, preventing further PTY polling after the shell has exited.

**No work required for ISSUE-005.**

---

## ISSUE-006: Cursor Invisible on Blank Cells

### Issue Description (from ISSUES.md)
File: `arcterm-render/src/text.rs:112-117` — Cursor on blank/space cells is invisible because the inverse-video approach only recolors the text glyph (whitespace renders nothing).

### Current State: **Partially addressed — needs clarification**

The ISSUES.md description refers to a "text-only inverse-video approach." However, the renderer currently uses a **two-layer cursor strategy**:

**Layer 1 — QuadRenderer** (`arcterm-render/src/renderer.rs:418-429`):

```rust
// renderer.rs:418-429 — build_quad_instances()
if is_cursor {
    let block_color = if matches!(eff_fg, TermColor::Default) {
        palette.cursor_f32()
    } else {
        term_color_to_f32(eff_fg, true, palette)
    };
    quads.push(QuadInstance {
        rect: [x, y, cell_w, cell_h],
        color: block_color,
    });
}
```

A solid-color rectangle quad is drawn at the cursor position using the foreground color (or `palette.cursor_f32()` when the foreground is default). This quad covers the full cell regardless of whether the cell contains a character or a space.

**Layer 2 — TextRenderer** (`arcterm-render/src/text.rs:652-675`):

```rust
// text.rs:652-675 — shape_row_into_buffer()
let span_strings: Vec<(String, Color)> = row
    .iter()
    .map(|cell| {
        let s = cell.c.to_string();
        let fg = if cell.attrs.reverse {
            ansi_color_to_glyphon(cell.attrs.bg, false, palette)
        } else {
            ansi_color_to_glyphon(cell.attrs.fg, true, palette)
        };
        (s, fg)
    })
    .collect();
```

The text layer renders each cell's character with the cell's foreground color. It does not have special cursor-position logic. The cursor position is not passed into `shape_row_into_buffer`.

**Analysis:** The quad-layer cursor block already draws a visible solid rectangle at the cursor cell. If the text glyph for a space character renders as zero pixels (which it typically does for monospace fonts), the cursor block behind it is still fully visible because the text layer does not overdraw the background. The original ISSUES.md concern was valid for a pure inverse-video-text implementation, but the quad cursor block addresses it directly.

**Remaining question:** When the cursor sits on a blank cell, the quad is colored using `eff_fg` — the foreground color of that cell. For a blank cell with `Color::Default` foreground, this becomes `palette.cursor_f32()`. This should be visible. However, if the cell's foreground is the same as the background (e.g., both are the terminal background color), the cursor block would be invisible. This edge case can occur when the cell has an explicit fg = black on a black background.

**The CONTEXT-10.md decision** (render cursor as U+2588 block glyph for blank cells) would ensure the text layer also shows something, but this redundancy is not strictly necessary if the quad cursor is already working. The CONTEXT-10.md approach is lower-risk since it does not touch the `QuadRenderer` or cursor color logic.

**Work required for ISSUE-006:** Apply the CONTEXT-10.md decision: in `shape_row_into_buffer`, substitute the blank/space cell character with `'\u{2588}'` (U+2588, FULL BLOCK) when that cell is the cursor cell. This requires passing the cursor column index into `shape_row_into_buffer` — a signature change. Alternatively, the cell can be pre-substituted at the call site in `prepare_grid` / `prepare_grid_at` before `shape_row_into_buffer` is called.

### Integration Points for ISSUE-006

The `shape_row_into_buffer` function signature is:
```rust
// arcterm-render/src/text.rs:646-650
fn shape_row_into_buffer(
    buf: &mut Buffer,
    row: &[arcterm_core::Cell],
    font_system: &mut FontSystem,
    palette: &RenderPalette,
)
```

This function is called from two sites:
- `prepare_grid` at line 189 — single-pane path, has `cursor` in scope
- `prepare_grid_at` at line 279 — multi-pane path, has `cursor` in scope

Both callers already have the cursor position available (`let cursor = grid.cursor;` at lines 166 and 244 respectively). Passing `cursor` and the current `row_idx` into the function allows it to substitute the blank cell when `row_idx == cursor.row` and the character at `cursor.col` is a space.

---

## Test Coverage Audit

### Existing Tests in Scope

**`arcterm-app/src/input.rs`** — 14 tests covering:
- Arrow keys (normal and app cursor mode)
- Home/End (normal and app cursor mode)
- PageUp/PageDown, Delete (mode-independent)
- Enter, F1 (mode-independent)

Missing tests (required by Phase 10 success criteria):
- `ctrl_backslash_sends_0x1c` — verify Ctrl+`\` → `[0x1c]`
- `ctrl_bracket_right_sends_0x1d` — verify Ctrl+`]` → `[0x1d]`

Note: Testing `translate_key_event` with Ctrl modifier requires constructing a `winit::event::KeyEvent` with `ModifiersState::CONTROL`. The existing tests only exercise `translate_named_key` which bypasses the ctrl modifier path. New tests must construct a full `KeyEvent` or extract the ctrl arm into a testable unit function (analogous to the existing `translate_named_key` helper).

**`arcterm-render/src/text.rs`** — 5 tests covering hash row behaviors:
- `hash_row_identical_rows_produce_same_hash`
- `hash_row_different_chars_produce_different_hash`
- `hash_row_cursor_column_movement_invalidates`
- `hash_row_cursor_movement_other_row_unchanged`
- `hash_row_reverse_flag_invalidates`

For ISSUE-006, a new test is needed:
- `cursor_on_blank_cell_uses_block_glyph` — construct a grid row of spaces with the cursor at column 0, call `shape_row_into_buffer` (or the equivalent pre-substitution logic), assert the first cell character is `'\u{2588}'`.

**`arcterm-app/src/main.rs`** — No `#[cfg(test)]` block found. The file contains only integration-level code (the event loop). Unit-testable logic is factored into separate modules.

### No Tests Currently in main.rs

The Phase 10 success criteria require "each fix includes at least one regression test." For the MIGRATION and ISSUE-005 (which are already implemented), tests that verify the behavior at the unit level are needed in the relevant helper modules. For ISSUE-006, a unit test in `text.rs` is straightforward. For the MIGRATION, a unit test exercising `Grid::set_scroll_offset` via the `Terminal` wrapper already exists in `arcterm-core`.

---

## Dependencies Between Changes

```
MIGRATION (terminal.rs + main.rs)
    └── Prerequisite for: any cargo test -p arcterm-app to succeed
         (arcterm-app won't compile until MIGRATION is complete)

ISSUE-006 (arcterm-render/src/text.rs)
    ├── Requires: shape_row_into_buffer signature change OR call-site substitution
    └── Does not depend on MIGRATION (arcterm-render compiles clean today)

ISSUE-002, ISSUE-003, ISSUE-004, ISSUE-005
    └── Already implemented. Tests only.
```

**Recommended implementation order:**
1. MIGRATION first — unblocks compilation and testing of everything else.
2. ISSUE-006 — isolated to `text.rs`, can be developed independently once MIGRATION unblocks test runs.
3. Missing regression tests for ISSUE-002, ISSUE-003, ISSUE-004, ISSUE-005 — straightforward additions to existing test modules.

---

## Implementation Risk Notes

### MIGRATION Risk: MouseWheel clamping arithmetic

The current MouseWheel handler reads `grid.scrollback.len()` to clamp the new offset before assigning it. The new `set_scroll_offset()` API performs the same clamp internally. If the caller also passes a pre-clamped value, there is no double-clamping issue. If the caller passes a negative-biased value that was computed without the cap, the setter will silently saturate it. The arithmetic `(current - lines * N).max(0)` is safe because it can never exceed the unclamped current value; the setter will always accept it. No precision loss.

### ISSUE-006 Risk: Cursor glyph affects text hash

The `hash_row` function at line 727 hashes `cell.c` for each cell in the row. If the substitution is applied before hashing (i.e., to the actual row data rather than at render time), the hash will differ from the stored hash and every cursor-row reshape will be triggered correctly. If the substitution is applied _only_ inside `shape_row_into_buffer` without touching the hash path, the hash may not invalidate when the cursor moves away from a blank cell to a non-blank cell (or vice versa), causing stale glyph cache entries. The safest implementation is a call-site substitution applied to a temporary `Cell` before passing to `shape_row_into_buffer`, with no changes to the `Cell` stored in the grid.

### No Risk: ISSUE-002, ISSUE-003, ISSUE-004, ISSUE-005

These are already implemented. Regression tests are additive and carry no structural risk.

---

## Open Questions

1. **ISSUE-006 cursor quad color edge case:** If a terminal application explicitly sets the foreground color of a blank cell to the palette background color (producing a "hidden" cell), the cursor quad will be invisible because `block_color` is computed from `eff_fg`. The CONTEXT-10.md glyph substitution does not help here either. This edge case was not raised in ISSUES.md and may be deferred to the dedicated wgpu quad pass (v0.2.0). Flagged for planner awareness.

2. **Ctrl+\ test infrastructure:** The `translate_key_event` function requires a `&KeyEvent` struct. `winit::event::KeyEvent` is non-trivially constructible in tests (requires `PhysicalKey`, `ElementState`, etc.). It may be practical to extract the ctrl character mapping into a helper `fn ctrl_byte(ch: char) -> Option<Vec<u8>>` that can be unit-tested without winit types. This is a refactor; the planner should decide if it is in scope for Phase 10 or if manual integration testing is sufficient for the Ctrl+\ regression.

3. **`terminal.rs::set_scroll_offset` method retention:** The `Terminal::set_scroll_offset` wrapper is marked `#[allow(dead_code)]` (line 196). After the fix delegates to `Grid::set_scroll_offset`, the method remains useful as a stable API surface for callers that hold a `Terminal` rather than a raw `Grid`. It should be kept and the dead_code allow removed once it is called from somewhere (or retained with the allow if it is expected to be used in a future phase).

---

## Sources

All findings are from direct codebase inspection. No external research was required for this phase.

- `arcterm-app/src/main.rs` — primary investigation site
- `arcterm-app/src/input.rs` — ISSUE-003 investigation
- `arcterm-app/src/terminal.rs` — MIGRATION terminal.rs error site
- `arcterm-render/src/text.rs` — ISSUE-006 text rendering
- `arcterm-render/src/renderer.rs` — ISSUE-006 quad cursor rendering
- `arcterm-core/src/grid.rs` — Phase 9 accessor API (set_scroll_offset, scroll_offset)
- `.shipyard/ISSUES.md` — issue descriptions
- `.shipyard/ROADMAP.md:302-328` — Phase 10 scope and success criteria
- `.shipyard/phases/10/CONTEXT-10.md` — design decisions
- `.shipyard/phases/9/results/SUMMARY-1.1.md` — Phase 9 scroll_offset encapsulation details
- `.shipyard/codebase/ARCHITECTURE.md` — layer structure and AppState overview
- `.shipyard/codebase/CONVENTIONS.md` — error handling patterns, test conventions
