# SUMMARY-3.1: Rewire Renderer to Read Alacritty's Grid

## Status: Complete

All three tasks executed sequentially. Full workspace compiles. 322 + 41 = 363 tests pass.

---

## Task 1: Replace Grid/Cell types in renderer with alacritty equivalents

**Commit:** `shipyard(phase-12): task 1 — replace Grid/Cell with RenderSnapshot in renderer`

### What was done

Created `arcterm-render/src/snapshot.rs` with three public types and one public function:

- `SnapshotColor` — enum with `Default`, `Indexed(u8)`, `Rgb(u8, u8, u8)` variants, mapping
  directly from `vte::ansi::Color` (`Named` → `Indexed` via `as u8`, `Indexed` → `Indexed`,
  `Spec(Rgb)` → `Rgb`)
- `SnapshotCell` — per-cell struct with `c: char`, `fg`, `bg`, `bold`, `italic`, `underline`,
  `inverse`
- `RenderSnapshot` — flat row-major cell buffer plus `cols`, `rows`, `cursor_row`, `cursor_col`,
  `cursor_visible`, `cursor_shape` (`CursorShape::Hidden` detected from `cursor.shape` field)
- `snapshot_from_term<E: EventListener>(term: &Term<E>) -> RenderSnapshot` — iterates
  `term.renderable_content().display_iter`, maps each `Indexed<&Cell>` into a `SnapshotCell`
  at `row * cols + col`, then reads cursor from `content.cursor.shape` and `.point`

Updated `arcterm-render/Cargo.toml`: removed `arcterm-core`, added `alacritty_terminal.workspace`
and `vte.workspace`.

Updated `renderer.rs`:
- `PaneRenderInfo.grid: &Grid` → `PaneRenderInfo.snapshot: &RenderSnapshot`
- `render_frame(&Grid)` → `render_frame(&RenderSnapshot)`
- `build_quad_instances_at(&Grid, ...)` → `build_quad_instances_at(&RenderSnapshot, ...)`
  iterates `row_idx in 0..snapshot.rows` / `col_idx in 0..snapshot.cols`
- `term_color_to_f32` now takes `SnapshotColor` instead of `TermColor`
- `grid_size_for_window` now returns `(usize, usize)` (rows, cols) instead of `GridSize`
- Removed `use arcterm_core::{Color as TermColor, Grid, GridSize}`

Updated `lib.rs` to expose `snapshot` module and re-export the new types.

---

## Task 2: Update TextRenderer for SnapshotCell

**Commit:** `shipyard(phase-12): task 2 — update TextRenderer and tests for SnapshotCell`

### What was done

Updated `text.rs`:
- Removed `use arcterm_core::{Color as TermColor, Grid}`, added `use crate::snapshot::{...}`
- `prepare_grid` and `prepare_grid_at` now take `&RenderSnapshot`; rows extracted via
  `snapshot.row(row_idx)` returning `&[SnapshotCell]`
- `hash_row` signature changed from `(&[arcterm_core::Cell], usize, CursorPos)` to
  `(&[SnapshotCell], usize, Option<usize>)` — cursor column is now an optional usize
  instead of a `CursorPos` struct
- `substitute_cursor_char` now takes `&[SnapshotCell]`
- `shape_row_into_buffer` now takes `&[SnapshotCell]`; attributes read as `cell.fg`, `cell.bg`,
  `cell.inverse` instead of `cell.attrs.*`
- `ansi_color_to_glyphon` now takes `SnapshotColor` instead of `TermColor`
- `hash_color` renamed to `hash_snapshot_color` to match new type
- All 9 unit tests updated to use `SnapshotCell::default()` and `SnapshotColor` variants

Rewrote `arcterm-render/examples/window.rs`: replaced `arcterm_core::{Cell, CellAttrs, Color,
Grid, GridSize}` with `arcterm_render::{RenderSnapshot, SnapshotCell, SnapshotColor}`;
`build_test_grid` replaced by `build_test_snapshot` building a flat cell Vec directly;
`grid_size_for_window` used with tuple destructuring; `render_frame` receives `&RenderSnapshot`.

---

## Task 3: Wire snapshot extraction into AppState render path

**Commit:** `shipyard(phase-12): task 3 — wire snapshot_from_term into AppState render path`

### What was done

**terminal.rs** — Added `lock_term()` method returning
`impl std::ops::Deref<Target = Term<ArcTermEventListener>> + '_`. This was specified by the plan
but not implemented in PLAN-2.1; it was added here as the minimal unblocking fix (inline deviation
documented below).

**main.rs** — Three `to_arcterm_grid()` call sites replaced:

1. **Shell-exited banner path**: `terminal.to_arcterm_grid()` replaced with
   `snapshot_from_term(&*terminal.lock_term())`. Banner text written by mutating
   `snapshot.cells[row_start + col]` directly using `SnapshotColor::Indexed(11/0)`.

2. **Normal multi-pane render path**: `pane_frames` type changed from
   `Vec<(PixelRect, arcterm_core::Grid, ...)>` to
   `Vec<(PixelRect, arcterm_render::RenderSnapshot, ...)>`. Each terminal locked briefly,
   snapshot extracted, lock released before GPU work begins.

3. **Clipboard copy (Cmd+C)**: `to_arcterm_grid()` replaced with `snapshot_from_term`.

4. **Auto-detect path**: `terminal.grid_cells_for_detect()` replaced with
   `snapshot_from_term(&*terminal.lock_term())` passed directly to `detector.scan_rows`.

**spawn helpers** — `spawn_default_pane`, `spawn_pane`, `spawn_pane_with_cwd` updated to accept
`(usize, usize)` (rows, cols) instead of `GridSize` since `grid_size_for_window` now returns a
tuple. Call sites using `grid_size_for_rect` (which still returns `GridSize`) convert with
`(new_size.rows, new_size.cols)`.

**Imports** — `use arcterm_core::{Cell, CellAttrs, Color, CursorPos, GridSize}` trimmed to
`use arcterm_core::GridSize` (GridSize still used by `grid_size_for_rect` and resize paths).

**selection.rs** — `extract_text` updated to take `&RenderSnapshot`, `word_boundaries` updated
to take `&[SnapshotCell]`. All 12 selection tests updated; helper `make_grid_with_text` replaced
by `make_snapshot_with_text`.

**detect.rs** — `scan_rows` updated to take `&RenderSnapshot`. Character extraction now uses
`snapshot.row(r).iter().map(|c| c.c)`. Test helper `rows_from_strings` now returns `RenderSnapshot`
built from a flat cell Vec. All 12 detection tests pass unchanged in logic.

---

## Deviations

### `lock_term()` not present from PLAN-2.1

The plan assumed `Terminal::lock_term()` was delivered by PLAN-2.1. It was not — only
`with_term()` / `with_term_mut()` closures existed. Added `lock_term()` returning
`impl Deref<Target = Term<...>> + '_` using the `opaque impl Trait` pattern to avoid
importing `parking_lot` into the public API. This is the minimum to unblock the plan.

### `FairMutexGuard` does not exist

`alacritty_terminal::sync::FairMutexGuard` is not a real type — `FairMutex::lock()` returns
`parking_lot::MutexGuard`. Used opaque `impl Deref` return type to sidestep the visibility issue.

### `SnapshotColor::Default` for named colors

Named colors from alacritty (`VteColor::Named(n)`) are mapped to `SnapshotColor::Indexed(n as u8)`
rather than a separate `Named` variant. This matches how the existing `to_arcterm_grid()` bridge
mapped them and how the palette's `indexed_glyphon(n)` function handles them. Terminal named colors
(black=0, red=1, ...) correspond directly to 256-color palette indices 0-15.

---

## Verification

```
cargo check --workspace  →  0 errors
cargo test -p arcterm-render  →  41 passed, 0 failed
cargo test -p arcterm-app     →  322 passed, 0 failed
```

The render path now follows: `lock_term()` → `snapshot_from_term()` → `unlock` → build quads +
shape text → submit to GPU. The `FairMutex` is held only for microseconds during snapshot
extraction, not during GPU command encoding or submission.
