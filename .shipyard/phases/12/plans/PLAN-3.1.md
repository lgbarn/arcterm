# Plan 3.1: Rewire Renderer to Read Alacritty's Grid

## Context

The renderer (`arcterm-render`) currently reads from `arcterm_core::Grid` via `grid.rows_for_viewport()` (a 2D row-slice API) and uses `arcterm_core::{Cell, CellAttrs, Color, CursorPos}`. After Wave 2, the Terminal wrapper provides access to `alacritty_terminal::Term` via `lock_term()`. The renderer must switch to using `term.renderable_content()` which provides a flat `display_iter` over `Indexed<&Cell>` plus cursor and mode information.

This is the most impactful change to `arcterm-render`. Every function that touches Grid, Cell, or Color must be updated.

## Dependencies

- Plan 2.1 (Terminal wrapper is functional, `lock_term()` available)

## Tasks

### Task 1: Replace Grid/Cell types in renderer with alacritty equivalents
**Files:** `arcterm-render/src/renderer.rs`, `arcterm-render/src/text.rs`, `arcterm-render/Cargo.toml`
**Action:** refactor
**Description:**

**1. Update arcterm-render/Cargo.toml:**
- Add `alacritty_terminal.workspace = true` to dependencies
- Add `vte.workspace = true` to dependencies (needed for `vte::ansi::Color` type)
- Remove `arcterm-core` dependency entirely

**2. Define a RenderSnapshot type** in a new file `arcterm-render/src/snapshot.rs`:
The renderer must NOT hold the FairMutex lock during the entire frame render (this would block the EventLoop). Instead, the app layer locks the Term, extracts a snapshot, unlocks, then passes the snapshot to the renderer.

```rust
pub struct RenderSnapshot {
    pub cells: Vec<SnapshotCell>,  // flat list, row-major order
    pub cols: usize,
    pub rows: usize,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub cursor_visible: bool,
    pub cursor_shape: CursorShape,
}

pub struct SnapshotCell {
    pub c: char,
    pub fg: SnapshotColor,
    pub bg: SnapshotColor,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
}

pub enum SnapshotColor {
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}
```

Add a `pub fn snapshot_from_term(term: &Term<impl EventListener>) -> RenderSnapshot` function that:
1. Calls `term.renderable_content()`
2. Iterates `content.display_iter`, converting each `Indexed<&Cell>` to `SnapshotCell`
3. Maps `vte::ansi::Color` to `SnapshotColor` (the exact Color enum variant names must be verified from vte 0.15 source — likely `Color::Named(NamedColor)`, `Color::Indexed(u8)`, `Color::Spec(Rgb)`)
4. Maps `cell.flags` to booleans (bold, italic, underline, inverse)
5. Extracts cursor from `content.cursor.point` and `content.mode.contains(TermMode::SHOW_CURSOR)`
6. Returns the snapshot

This decouples the renderer from alacritty's lock semantics and from the specific Cell/Color types.

**3. Update PaneRenderInfo:**
In `renderer.rs`, change:
```rust
pub struct PaneRenderInfo<'a> {
    pub grid: &'a Grid,  // was arcterm_core::Grid
    ...
}
```
to:
```rust
pub struct PaneRenderInfo<'a> {
    pub snapshot: &'a RenderSnapshot,
    ...
}
```

**4. Update build_quad_instances_at:**
Change from iterating `grid.rows_for_viewport()` to iterating `snapshot.cells` as a 2D grid using `snapshot.cols` as stride:
```rust
for row_idx in 0..snapshot.rows {
    for col_idx in 0..snapshot.cols {
        let cell = &snapshot.cells[row_idx * snapshot.cols + col_idx];
        // ... same quad logic but using SnapshotColor instead of TermColor
    }
}
```
Update `term_color_to_f32` to match on `SnapshotColor` variants.

**5. Update grid_size_for_window:**
This currently returns `GridSize`. Change to return `(usize, usize)` — `(rows, cols)`. The caller in AppState converts to `WindowSize`.

**6. Update render_frame and render_multipane:**
Change parameter from `&Grid` to `&RenderSnapshot`.

**Acceptance Criteria:**
- `arcterm-render/Cargo.toml` no longer depends on `arcterm-core` or `arcterm-vt`
- `cargo check -p arcterm-render` succeeds
- `build_quad_instances_at` works with `RenderSnapshot`
- `render_frame` and `render_multipane` accept `PaneRenderInfo` with snapshot

### Task 2: Update TextRenderer for SnapshotCell
**Files:** `arcterm-render/src/text.rs`
**Action:** refactor
**Description:**

**1. Update prepare_grid and prepare_grid_at:**
Change from `grid: &Grid` parameter to `snapshot: &RenderSnapshot`. Instead of `grid.rows_for_viewport()` returning `&[&[Cell]]`, iterate the snapshot's flat cell array in row-major order.

For each row:
```rust
let row_start = row_idx * snapshot.cols;
let row_cells = &snapshot.cells[row_start..row_start + snapshot.cols];
```

**2. Update shape_row_into_buffer:**
Change signature from `row: &[arcterm_core::Cell]` to `row: &[SnapshotCell]`. Update the per-cell attribute access:
- `cell.c` → `cell.c` (same)
- `cell.attrs.fg` → `cell.fg` (SnapshotColor)
- `cell.attrs.bg` → `cell.bg` (SnapshotColor)
- `cell.attrs.reverse` → `cell.inverse`
- `cell.attrs.bold` → `cell.bold`

**3. Update ansi_color_to_glyphon:**
Change from `TermColor` to `SnapshotColor`:
```rust
pub fn ansi_color_to_glyphon(color: SnapshotColor, is_fg: bool, palette: &RenderPalette) -> Color {
    match color {
        SnapshotColor::Default => if is_fg { palette.fg_glyphon() } else { ... },
        SnapshotColor::Rgb(r, g, b) => Color::rgb(r, g, b),
        SnapshotColor::Indexed(n) => palette.indexed_glyphon(n),
    }
}
```

**4. Update substitute_cursor_char:**
Change from `row: &[arcterm_core::Cell]` to `row: &[SnapshotCell]`.

**5. Update hash_row:**
Change from `row: &[arcterm_core::Cell], cursor: arcterm_core::CursorPos` to `row: &[SnapshotCell], row_idx: usize, cursor_col: Option<usize>`.

**6. Update tests:**
All tests in `text.rs` use `arcterm_core::{Cell, Color, CursorPos}`. Rewrite them to use `SnapshotCell` and `SnapshotColor`.

**7. Update the example:**
`arcterm-render/examples/window.rs` uses `arcterm_core` types. Update to use `RenderSnapshot`.

**Acceptance Criteria:**
- `cargo check -p arcterm-render` succeeds with no `arcterm_core` imports remaining
- `cargo test -p arcterm-render` passes (all tests updated for new types)
- Text shaping produces correct output for SnapshotCell input

### Task 3: Wire snapshot extraction into AppState render path
**Files:** `arcterm-app/src/main.rs`
**Action:** modify
**Description:**

**1. Add snapshot extraction before render:**
In `about_to_wait`, after processing wakeups and before calling `render_multipane`, extract a `RenderSnapshot` for each pane:
```rust
let snapshot = {
    let term = terminal.lock_term();
    arcterm_render::snapshot_from_term(&*term)
};
```
The lock is held only for the duration of the snapshot extraction (microseconds), not the entire frame render.

**2. Build PaneRenderInfo with snapshots:**
```rust
let pane_info = PaneRenderInfo {
    snapshot: &snapshot,
    rect: pane_rect,
    structured_blocks: &blocks,
};
```

**3. Update selection.rs:**
Selection currently operates on `&Grid` (arcterm_core). Update to work with `RenderSnapshot` or extract text from the alacritty Term's grid directly. The selection module needs character data at (row, col) positions — this is available from the snapshot.

**4. Update detect.rs auto-detection:**
Auto-detection reads cell characters to detect code blocks, diffs, etc. Update to read from `RenderSnapshot.cells` instead of `arcterm_core::Cell`.

**Acceptance Criteria:**
- `cargo check -p arcterm-app` succeeds
- `cargo test -p arcterm-app` passes
- Renderer receives snapshots extracted from alacritty's Term
- FairMutex lock is held only during snapshot extraction, not during GPU rendering
- Selection and auto-detection work with the new types

## Verification

```bash
cargo check --workspace && cargo test -p arcterm-render -p arcterm-app
```

The entire workspace compiles. Renderer tests pass with new types. App tests pass. The render path: lock Term -> extract snapshot -> unlock -> build quads + text -> submit to GPU.
