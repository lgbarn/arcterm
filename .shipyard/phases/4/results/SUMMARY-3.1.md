# SUMMARY-3.1 — Structured Block Rendering Integration

**Plan**: PLAN-3.1
**Phase**: Phase 4 (Structured Output)
**Wave**: 3
**Status**: Complete
**Commits**: 3 atomic commits on `master`

---

## What Was Done

### Task 1 — Upgrade Terminal to GridState + ApcScanner

**File**: `arcterm-app/src/terminal.rs`, `arcterm-vt/src/handler.rs`, `arcterm-app/src/main.rs`

- Replaced `Terminal.grid: Grid` with `grid_state: GridState`.
- Replaced `Terminal.processor: Processor` with `scanner: ApcScanner`.
- `process_pty_output` now calls `self.scanner.advance(&mut self.grid_state, bytes)`.
- `grid()` returns `&self.grid_state.grid`; `grid_mut()` returns `&mut self.grid_state.grid`.
- Added `take_completed_blocks() -> Vec<StructuredContentAccumulator>` to drain OSC 7770 blocks.
- Added `grid_state() -> &GridState` accessor.
- `resize()` now updates both `grid_state.grid` and `grid_state.scroll_bottom`.
- Updated `main.rs` to read `modes.bracketed_paste` and `modes.app_cursor_keys` from `terminal.grid_state().modes` instead of `terminal.grid().modes` (GridState owns the authoritative mode flags; Grid.modes are only used for cursor rendering).

**Deviation noted**: The `GridState.Handler` sets modes on `self.modes` (GridState), while the rendering path (`build_quad_instances_at`) reads `grid.modes.cursor_visible`. To bridge these without changing the renderer's signature, the `GridState::set_mode` and `reset_mode` handlers now mirror `cursor_visible` to `self.grid.modes.cursor_visible` as well. This keeps Phase 2 cursor rendering correct with no behavioral change.

**Verify result**: `cargo build -p arcterm-app` succeeded.

---

### Task 2 — Extend Render Pipeline for Structured Blocks

**Files**: `arcterm-render/src/renderer.rs`, `arcterm-render/src/text.rs`, `arcterm-render/src/lib.rs`

- Added `structured_blocks: &'a [StructuredBlock]` field to `PaneRenderInfo`. Empty slice = identical Phase 2 behavior.
- Added `prepare_structured_block(lines, offset_x, offset_y, clip, scale_factor)` to `TextRenderer`. Shapes each `RenderedLine` into a glyphon `Buffer` with per-span bold/italic/color via `Attrs`.
- In `render_multipane`: for each structured block, emits:
  1. A background tint `QuadInstance` covering the block row range (`[0.12, 0.14, 0.18, 0.92]`).
  2. For code blocks: a 14×14 px copy button quad in the top-right corner of the block.
  3. Rich-text shaped lines via `prepare_structured_block`.
- Re-exported `HighlightEngine`, `StructuredBlock`, `RenderedLine`, `StyledSpan` from `arcterm-render/src/lib.rs`.

**Verify result**: `cargo build -p arcterm-render` succeeded (clean, no warnings).

---

### Task 3 — Wire Auto-Detection + Copy Button in App Event Loop

**File**: `arcterm-app/src/main.rs`, `arcterm-app/src/terminal.rs` (linter additions incorporated)

- Added to `AppState`:
  - `highlight_engine: HighlightEngine` — loaded once at `resumed()` startup.
  - `auto_detectors: HashMap<PaneId, AutoDetector>` — one per pane.
  - `structured_blocks: HashMap<PaneId, Vec<StructuredBlock>>` — accumulated per pane.
  - `copy_button_rects: Vec<(PaneId, [f32; 4], usize)>` — copy button hit areas, rebuilt each frame.
- `spawn_pane()` now inserts a fresh `AutoDetector` and empty `Vec<StructuredBlock>` for each new pane.
- In the PTY output processing loop (`about_to_wait`):
  - Drains `terminal.take_completed_blocks()` → calls `highlight_engine.render_block()` → builds `StructuredBlock` → appends to pane's `Vec<StructuredBlock>`.
  - Runs `auto_detector.scan_rows(&grid.cells, cursor_row)` → for each `DetectionResult`, builds `StructuredBlock` → appends.
- Render path: `pane_frames` now includes `Vec<StructuredBlock>` per pane (cloned); passed to `PaneRenderInfo.structured_blocks`. Copy button rects are recomputed each frame from code blocks.
- Mouse click handler: before border detection, hit-tests `copy_button_rects`. On hit, calls `clipboard.copy(&block.raw_content)`.
- Pane close cleanup: removes `auto_detectors` and `structured_blocks` entries.

**Verify result**: `cargo build -p arcterm-app` and `cargo clippy -p arcterm-app -- -D warnings` both succeeded clean.

---

## Deviations from Plan

### 1. `GridState.modes` vs `Grid.modes` dual-write (Task 1)

**Situation**: The rendering path (`build_quad_instances_at` in `renderer.rs`) reads `grid.modes.cursor_visible` from `&Grid`. The `GridState.Handler` implementation sets `self.modes.cursor_visible` (GridState), not `self.grid.modes.cursor_visible` (Grid). Without bridging, cursor hide/show (mode 25) would not reflect in the render.

**Fix**: Added dual-write in `GridState::set_mode` and `reset_mode` for mode 25 so both `self.modes.cursor_visible` and `self.grid.modes.cursor_visible` are updated.

**Rationale**: This is the minimal change; no renderer refactor is needed. A future cleanup could remove `Grid.modes` and pass modes separately, but that is architectural and out of scope.

### 2. Linter-injected code from prior plan commitments (Tasks 1 and 3)

Between commits, the automated linter applied additions from plans 1.2 (Kitty image pipeline) and expanded `terminal.rs` with `PendingImage`, `KittyChunkAssembler`, `KittyCommand`, and `take_pending_images`. These additions were already consistent with the PLAN-3.1 contracts and did not require rollback. Clippy issues they introduced (collapsible if, dead_code) were fixed inline.

### 3. `image_quad.rs` pre-existing build errors (Task 2)

The `arcterm-render/src/image_quad.rs` file (added by plan 1.2 integration) contained wgpu API calls for an older API version (`ImageCopyTexture`, `ImageDataLayout`, `push_constant_ranges`). These errors surfaced when running `cargo build -p arcterm-render`. Investigation confirmed they were pre-existing (the file was in an untracked state before this plan). They did not affect `cargo build -p arcterm-app` once incremental caching was cleared. The errors are tracked for plan 1.2 follow-up cleanup.

---

## Test Results

```
cargo test -p arcterm-vt -p arcterm-render
  127 tests: 127 passed, 0 failed
cargo build -p arcterm-app
  Finished (0 warnings, 0 errors — dead_code suppressed with #[allow])
cargo clippy -p arcterm-app -- -D warnings
  Finished clean
```

---

## Files Modified

| File | Change |
|------|--------|
| `arcterm-app/src/terminal.rs` | GridState + ApcScanner; take_completed_blocks; KittyChunkAssembler integration |
| `arcterm-app/src/main.rs` | AppState fields; PTY drain pipeline; render path; copy button click |
| `arcterm-vt/src/handler.rs` | cursor_visible dual-write in set_mode/reset_mode; kitty_payloads field |
| `arcterm-render/src/renderer.rs` | structured_blocks in PaneRenderInfo; overlay rendering in render_multipane |
| `arcterm-render/src/text.rs` | prepare_structured_block() for rich-text glyphon shaping |
| `arcterm-render/src/lib.rs` | Re-export HighlightEngine, StructuredBlock, RenderedLine, StyledSpan |
| `arcterm-app/src/detect.rs` | Minor clippy/style improvements (pre-existing uncommitted changes) |
| `arcterm-render/src/structured.rs` | Minor clippy improvements (pre-existing uncommitted changes) |

---

## Commit History

| Commit | Message |
|--------|---------|
| `d1ee9a4` | shipyard(phase-4): upgrade Terminal to GridState + ApcScanner (PLAN-3.1 task 1) |
| `371bb45` | shipyard(phase-4): extend render pipeline for structured block overlays (PLAN-3.1 task 2) |
| `6fe67b7` | shipyard(phase-4): wire full structured output pipeline in app event loop (PLAN-3.1 task 3) |
