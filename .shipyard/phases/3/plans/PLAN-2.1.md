---
phase: multiplexer
plan: "2.1"
wave: 2
dependencies: ["1.1", "1.2"]
must_haves:
  - Renderer accepts multiple grids with pixel rect offsets
  - TextRenderer.prepare_grid_at() takes offset_x, offset_y parameters
  - build_quad_instances_at() offsets all quads by pane pixel rect origin
  - Border quads rendered between panes with focus indicator color
  - Tab bar rendered as quads + text at top of window
  - Single render pass handles all panes, borders, and tab bar
files_touched:
  - arcterm-render/src/renderer.rs
  - arcterm-render/src/text.rs
  - arcterm-render/src/lib.rs
tdd: false
---

# PLAN-2.1 -- Multi-Pane Rendering Pipeline

## Goal

Extend the renderer to draw N pane grids at arbitrary pixel offsets within a single frame, plus border lines between panes and a tab bar at the top. This is the visual foundation of the multiplexer -- without it, pane trees are invisible.

## Why Wave 2

This plan depends on PLAN-1.1 (`PixelRect`, `BorderQuad`) for the rect types used to position each pane's rendering. The renderer does not directly import `PaneNode`, but the calling code (Wave 3) will pass rects computed by `compute_rects()`.

## Tasks

<task id="1" files="arcterm-render/src/text.rs" tdd="false">
  <action>Add a `prepare_grid_at()` method to `TextRenderer` that renders a grid at an arbitrary pixel offset:

1. Add method signature:
   ```rust
   pub fn prepare_grid_at(
       &mut self,
       device: &wgpu::Device,
       queue: &wgpu::Queue,
       grid: &Grid,
       scale_factor: f32,
       palette: &RenderPalette,
       offset_x: f32,
       offset_y: f32,
       clip_width: f32,
       clip_height: f32,
   ) -> Vec<TextArea<'_>>
   ```

   This method follows the same logic as the existing `prepare_grid`, but:
   - Each `TextArea.left` is set to `offset_x` (instead of `0.0`).
   - Each `TextArea.top` is set to `offset_y + row_idx as f32 * cell_h` (instead of `row_idx as f32 * cell_h`).
   - Each `TextArea.bounds` is set to `TextBounds { left: (offset_x * scale_factor) as i32, top: (offset_y * scale_factor) as i32, right: ((offset_x + clip_width) * scale_factor) as i32, bottom: ((offset_y + clip_height) * scale_factor) as i32 }` to clip text within the pane rect.
   - The method returns `Vec<TextArea>` instead of calling `self.renderer.prepare()` directly. The caller collects all TextAreas from all panes and submits them in one `prepare()` call.

2. Refactor the row buffer pool: currently `row_buffers: Vec<Buffer>` is sized per single grid. For multi-pane, change to a flat pool approach: track `total_rows_used: usize` and grow `row_buffers` as needed. Each call to `prepare_grid_at` consumes rows from `row_buffers[total_rows_used..total_rows_used + grid_rows]` and increments the counter. Add `pub fn reset_frame(&mut self)` that sets `total_rows_used = 0` and clears `row_hashes` (since row buffer indices change between frames when pane count changes).

3. Add `pub fn submit_text_areas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, areas: Vec<TextArea<'_>>) -> Result<(), glyphon::PrepareError>` that calls `self.renderer.prepare(device, queue, &mut self.font_system, &mut self.atlas, &self.viewport, areas, &mut self.swash_cache)`.

4. Keep the existing `prepare_grid` method working (it calls `prepare_grid_at` with offset 0,0 and full viewport clip, then calls `submit_text_areas`). This maintains backward compatibility until the AppState restructuring in Wave 3.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-render 2>&1 | tail -10</verify>
  <done>`cargo build` succeeds. `prepare_grid_at` returns offset TextAreas with correct bounds clipping. Existing `prepare_grid` still works via delegation.</done>
</task>

<task id="2" files="arcterm-render/src/renderer.rs" tdd="false">
  <action>Add a multi-pane render method to `Renderer`:

1. Define `PaneRenderInfo` struct (in renderer.rs or re-exported):
   ```rust
   pub struct PaneRenderInfo<'a> {
       pub grid: &'a Grid,
       pub rect: [f32; 4],  // [x, y, width, height] in physical pixels
   }
   ```

2. Define `OverlayQuad` struct:
   ```rust
   pub struct OverlayQuad {
       pub rect: [f32; 4],
       pub color: [f32; 4],
   }
   ```

3. Add method `render_multipane`:
   ```rust
   pub fn render_multipane(
       &mut self,
       panes: &[PaneRenderInfo<'_>],
       overlay_quads: &[OverlayQuad],
       scale_factor: f64,
   )
   ```

   Implementation:
   a. Call `self.text.update_viewport(queue, w, h)`.
   b. Call `self.text.reset_frame()` to reset the row buffer pool.
   c. For each pane: call `build_quad_instances_at(grid, cell_size, sf, palette, rect)` and collect into a master quad vec. `build_quad_instances_at` is a new helper (see below) that offsets all quads by `rect[0], rect[1]`.
   d. Append all `overlay_quads` converted to `QuadInstance` format to the master quad vec.
   e. For each pane: call `self.text.prepare_grid_at(...)` with the pane's rect offset and collect all returned TextAreas into a single Vec.
   f. Call `self.quads.prepare(queue, &all_quads, w, h)`.
   g. Call `self.text.submit_text_areas(device, queue, all_text_areas)`.
   h. Execute the render pass (same as existing: begin_frame, begin_render_pass, quads.render, text.render, present).

4. Add helper `build_quad_instances_at(grid, cell_size, sf, palette, rect: [f32; 4]) -> Vec<QuadInstance>` -- same logic as existing `build_quad_instances` but each quad's x is offset by `rect[0]` and y is offset by `rect[1]`. The grid is rendered within the rect's dimensions.

5. Keep existing `render_frame` working by having it construct a single `PaneRenderInfo` spanning the full window and delegate to `render_multipane`.

6. Re-export `PaneRenderInfo` and `OverlayQuad` from `arcterm-render/src/lib.rs`.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-render && cargo clippy -p arcterm-render -- -D warnings 2>&1 | tail -10</verify>
  <done>`cargo build` and `cargo clippy` succeed. `render_multipane` compiles and `render_frame` still works via delegation. `PaneRenderInfo` and `OverlayQuad` are re-exported from the crate root.</done>
</task>

<task id="3" files="arcterm-render/src/renderer.rs, arcterm-render/src/lib.rs" tdd="false">
  <action>Add tab bar rendering support:

1. Add `pub fn render_tab_bar_quads(tabs: &[(String, bool)], cell_size: &CellSize, scale_factor: f32, window_width: f32, palette: &RenderPalette) -> Vec<QuadInstance>` as a free function in `renderer.rs`. Arguments: slice of `(label, is_active)` pairs. Produces:
   - One full-width background quad at y=0, height = `cell_size.height * scale_factor`, color = palette background slightly dimmed (multiply RGB by 0.7).
   - Per-tab quads: starting at x=0, each tab is `(label.len() + 4) * cell_size.width * scale_factor` wide. Active tab: palette foreground at 30% alpha. Inactive: transparent (no quad).
   - These quads are added to the `overlay_quads` list by the caller.

2. Add `pub fn tab_bar_height(cell_size: &CellSize, scale_factor: f32) -> f32` -- returns `cell_size.height * scale_factor`. This is used by the app layer to offset all pane rects below the tab bar.

3. For tab label text: add `pub fn prepare_tab_bar_text(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, tabs: &[(String, bool)], scale_factor: f32, palette: &RenderPalette) -> Vec<TextArea<'_>>` to `TextRenderer`. Creates one small `Buffer` per tab label, positioned at the correct x offset within the tab bar. Active tab text uses palette foreground; inactive uses palette foreground at 60% brightness.

4. Re-export `tab_bar_height` from `arcterm-render/src/lib.rs`.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-render && cargo clippy -p arcterm-render -- -D warnings 2>&1 | tail -10</verify>
  <done>Tab bar rendering functions compile and pass clippy. `tab_bar_height` is re-exported. The functions produce correct quad and text data for a given tab list.</done>
</task>
