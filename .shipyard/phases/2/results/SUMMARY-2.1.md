# SUMMARY-2.1.md — Phase 2 Plan 2.1: Background Color Rendering and Dirty-Row Optimization

**Branch:** master
**Date:** 2026-03-15
**Status:** Complete — all 3 tasks committed, 153 tests passing.

---

## Task 1: QuadRenderer wgpu colored quad pipeline

**Commit:** `36fee2f` — `shipyard(phase-2): add QuadRenderer wgpu colored quad pipeline`

**Files created/modified:**
- `arcterm-render/src/quad.rs` (new)
- `arcterm-render/src/lib.rs` (export `QuadInstance`, `QuadRenderer`)
- `arcterm-render/Cargo.toml` (add `bytemuck = { version = "1", features = ["derive"] }`)

**Implementation:**
- `QuadVertex`: position `[f32;2]`, color `[f32;4]`, derives `bytemuck::Pod` + `Zeroable`, provides `VertexBufferLayout` descriptor.
- `QuadInstance`: caller-facing rect `[f32;4]` + color `[f32;4]` struct (not Pod — stays on CPU until expanded to vertices).
- `QuadRenderer`: wgpu render pipeline (alpha blending), a vertex buffer sized for `MAX_QUADS = 8192` quads (6 vertices each), a resolution uniform buffer, and a bind group.
- WGSL shader: vertex stage converts pixel-space coords (origin = top-left, y-down) to clip space via `resolution` uniform; fragment stage outputs vertex color.
- `prepare(queue, quads, width, height)`: writes the resolution uniform, expands `QuadInstance` rects into two triangle fans, uploads vertices via `queue.write_buffer`.
- `render(pass)`: sets pipeline, bind group, vertex buffer, issues draw call.

**Deviations from plan:**
- wgpu 28 renamed `push_constant_ranges` on `PipelineLayoutDescriptor` (no longer exists; use `..Default::default()` instead) and renamed `multiview` to `multiview_mask: Option<NonZero<u32>>`. Both were fixed inline.

---

## Task 2: Integrate quads for cell backgrounds and cursor

**Commit:** `d57423b` — `shipyard(phase-2): integrate quad rendering for cell backgrounds and cursor`

**Files modified:**
- `arcterm-render/src/renderer.rs` — complete rewrite
- `arcterm-render/src/text.rs` — complete rewrite

**Implementation:**

`renderer.rs`:
- `Renderer` struct gains a `quads: QuadRenderer` field alongside `gpu` and `text`.
- `resize()` now also calls `self.text.row_hashes.clear()` (see Task 3).
- `render_frame()`: calls `build_quad_instances()`, uploads via `self.quads.prepare()`, then records `self.quads.render()` before `self.text.render()` in the single render pass.
- `build_quad_instances()`: iterates `grid.rows_for_viewport()` (not the old `grid.rows()`), reads `grid.modes.cursor_visible`, computes effective fg/bg with reverse-attribute handling, and emits:
  - A background `QuadInstance` for any cell whose effective bg is not `TermColor::Default`.
  - A cursor block `QuadInstance` at `cursor.row, cursor.col` when `cursor_visible && scroll_offset == 0` (cursor is not meaningful in scrollback rows).
- `term_color_to_f32()`: converts `TermColor` → `[f32;4]` by routing through the existing `ansi_color_to_glyphon()` helper.

`text.rs`:
- `prepare_grid()` switched from `grid.rows()` to `grid.rows_for_viewport()`.
- Inverse-video cursor hack (swapping fg/bg in text rendering) removed.
- Reverse attribute handled correctly: text draws the effective fg (which is the original bg when reversed), leaving the quad renderer to draw the correct background block.
- `row_hashes: Vec<u64>` field added to `TextRenderer`.
- `hash_row()` public helper added (also serves Task 3).
- Per-row skip logic added (also Task 3).

---

## Task 3: Dirty-row hash optimization

**Commit:** `876567e` — `shipyard(phase-2): add dirty-row hash optimization for text rendering`

**Files modified:**
- `arcterm-render/src/text.rs` (unit tests for `hash_row` added; cleanup of unused import)

**Implementation (landed during Task 2 text.rs rewrite, formalized here with tests):**
- `row_hashes: Vec<u64>` in `TextRenderer` — grown/shrunk in sync with `row_buffers` each frame.
- `hash_row(row, row_idx, cursor) -> u64`: uses `std::collections::hash_map::DefaultHasher`, hashes `row_idx`, cursor column only when `row_idx == cursor.row`, then each cell's char, fg, bg, bold, italic, underline, and reverse.
- Skip logic: `if !is_cursor_row && self.row_hashes[row_idx] == row_hash { continue; }` — saves `Buffer::set_rich_text` + `shape_until_scroll` for unchanged rows.
- Always re-shapes: cursor row is exempt from the hash skip unconditionally.
- Resize clear: `Renderer::resize()` calls `self.text.row_hashes.clear()`, ensuring all rows re-shape after a window resize.

**Unit tests (6):**
- `hash_row_identical_rows_match` — same row, same hash.
- `hash_row_char_change_invalidates` — char mutation changes hash.
- `hash_row_cursor_column_movement_invalidates` — cursor col change within cursor row changes hash.
- `hash_row_cursor_movement_other_row_unchanged` — cursor moving on a different row does not change row 0's hash.
- `hash_row_fg_color_change_invalidates` — fg color mutation changes hash.
- `hash_row_reverse_flag_invalidates` — reverse attribute flip changes hash.

---

## Verification

```
cargo build --workspace    → Finished (0 errors, 3 pre-existing warnings in arcterm-app)
cargo test --workspace     → 153 tests: 19 arcterm-app, 51 arcterm-core, 6 arcterm-pty,
                             6 arcterm-render, 71 arcterm-vt — all ok, 0 failures
```

---

## Deviations from Plan

| Task | Deviation | Resolution |
|------|-----------|------------|
| 1 | wgpu 28 removed `push_constant_ranges` from `PipelineLayoutDescriptor` | Used `..Default::default()` |
| 1 | wgpu 28 renamed `multiview` → `multiview_mask: Option<NonZero<u32>>` | Updated field name and type |
| 2/3 | Tasks 2 and 3 both modify `text.rs`; dirty-row infrastructure was written atomically during the Task 2 text.rs rewrite | Task 3 commit adds 6 unit tests for `hash_row`, giving the optimization its own independently verifiable artifact |

---

## Final State

Three new commits on `master`:

```
876567e  shipyard(phase-2): add dirty-row hash optimization for text rendering
d57423b  shipyard(phase-2): integrate quad rendering for cell backgrounds and cursor
36fee2f  shipyard(phase-2): add QuadRenderer wgpu colored quad pipeline
```

The renderer now:
1. Draws solid-color cell backgrounds and the cursor block via a wgpu triangle pipeline before text.
2. Handles `reverse` attribute by swapping effective fg/bg at the quad level, with text always rendered in the effective fg color.
3. Uses `rows_for_viewport()` throughout, correctly rendering scrollback.
4. Skips glyphon re-shaping for unchanged rows, with forced re-shape on the cursor row and after any resize.
