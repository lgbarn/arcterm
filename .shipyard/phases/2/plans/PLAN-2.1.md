---
phase: terminal-fidelity
plan: "2.1"
wave: 2
dependencies: ["1.1"]
must_haves:
  - wgpu pipeline for colored quads (cell backgrounds, cursor block, selection overlay)
  - Cell background colors render correctly for all Color variants
  - Cursor rendered as a solid colored block (not text-only inverse video)
  - Dirty-row optimization skips re-shaping unchanged rows
  - Renderer uses rows_for_viewport() to support scroll offset
files_touched:
  - arcterm-render/src/text.rs
  - arcterm-render/src/renderer.rs
  - arcterm-render/src/gpu.rs
  - arcterm-render/src/lib.rs
  - arcterm-render/src/quad.rs (new file)
  - arcterm-render/Cargo.toml
tdd: false
---

# PLAN-2.1 -- Background Color Rendering and Dirty-Row Optimization

## Goal

Add a wgpu geometry pass for colored rectangles (cell backgrounds, cursor block,
selection overlay) and implement dirty-row optimization to hit the 120 FPS target.
This plan also updates the renderer to use `rows_for_viewport()` from PLAN-1.1
for scrollback-aware rendering.

## Why Wave 2

Depends on PLAN-1.1 for `rows_for_viewport()`, `TermModes.cursor_visible`, and
the scrollback viewport. Does not depend on PLAN-1.2 (VT sequences) and does not
touch any VT files. Does not overlap with PLAN-2.2 (config) or PLAN-2.3
(mouse/selection/clipboard).

## Architecture Note

The quad pipeline is a simple vertex-colored rectangle renderer. Each quad is 6
vertices (2 triangles) with position (x, y) and color (r, g, b, a). The vertex
shader transforms from pixel coordinates to clip space. The fragment shader
passes through the vertex color. This is rendered BEFORE the glyphon text pass
so text appears on top of backgrounds.

## Tasks

<task id="1" files="arcterm-render/src/quad.rs, arcterm-render/Cargo.toml" tdd="false">
  <action>
  Create a new `quad.rs` module implementing a wgpu colored-quad pipeline.

  1. Define a `QuadVertex` struct with fields: `position: [f32; 2]`, `color: [f32; 4]`.
     Derive bytemuck::Pod and bytemuck::Zeroable (add `bytemuck = { version = "1", features = ["derive"] }` to arcterm-render/Cargo.toml).
  2. Define a `QuadInstance` struct: `rect: [f32; 4]` (x, y, width, height in pixels),
     `color: [f32; 4]` (rgba).
  3. Create `QuadRenderer` struct holding:
     - `pipeline: wgpu::RenderPipeline`
     - `vertex_buffer: wgpu::Buffer` (dynamic, re-uploaded each frame)
     - `uniform_buffer: wgpu::Buffer` (holds screen resolution for coordinate transform)
     - `bind_group: wgpu::BindGroup`
     - `max_quads: usize` (initial capacity, e.g. 8192)
  4. Implement `QuadRenderer::new(device, surface_format)` that:
     - Creates a WGSL shader module with a vertex shader that transforms pixel coords
       to clip space using a `resolution: vec2<f32>` uniform, and a fragment shader
       that outputs the vertex color.
     - Creates the pipeline layout, bind group layout, uniform buffer, vertex buffer,
       and render pipeline.
  5. Implement `QuadRenderer::prepare(device, queue, quads: &[QuadInstance], width, height)`
     that converts QuadInstances to vertices, writes to vertex buffer (recreating if
     capacity exceeded), and updates the resolution uniform.
  6. Implement `QuadRenderer::render(pass: &mut RenderPass)` that draws the vertex
     buffer.
  7. Add `pub mod quad;` to `arcterm-render/src/lib.rs`.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-render</verify>
  <done>arcterm-render builds cleanly. QuadRenderer struct and all methods compile. The WGSL shader compiles (verified by wgpu pipeline creation at build time via the render pipeline descriptor).</done>
</task>

<task id="2" files="arcterm-render/src/renderer.rs, arcterm-render/src/text.rs" tdd="false">
  <action>
  Integrate QuadRenderer into the render pipeline for cell backgrounds and cursor.

  1. Add `quad: QuadRenderer` field to the `Renderer` struct. Initialize it in
     `Renderer::new()`.
  2. In `render_frame()`, before the glyphon text prepare step:
     a. Iterate over the grid's visible rows (use `grid.rows_for_viewport()` if the
        method exists, otherwise fall back to `grid.rows()` for backward compat).
     b. For each cell where `cell.attrs.bg != Color::Default` OR `cell.attrs.reverse`
        is true (reverse swaps fg/bg), create a QuadInstance with the cell's background
        color converted to RGBA floats and positioned at (col * cell_w, row * cell_h,
        cell_w, cell_h).
     c. For the cursor cell (if `grid.modes.cursor_visible` is true or modes field
        does not exist yet), create a QuadInstance with a distinct cursor color
        (e.g., rgba(0.8, 0.8, 0.8, 0.8)) at the cursor position.
     d. Call `quad.prepare(device, queue, &quads, width, height)`.
  3. In the render pass, call `quad.render(&mut pass)` BEFORE `text.render(&mut pass)`.
  4. Update `text.rs` `prepare_grid()` to use `grid.rows_for_viewport()` instead of
     `grid.rows()` when available, to support scrollback viewport rendering.
  5. Remove the text-only inverse-video cursor hack from `text.rs` (the cursor cell
     no longer needs special fg color handling since the quad pipeline renders the
     cursor block).
  6. Update the `ansi_color_to_glyphon` usage: when a cell has `attrs.reverse`, swap
     fg and bg for the text color as well.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-app</verify>
  <done>arcterm-app builds and links cleanly. The render_frame method calls quad.prepare then quad.render before text.render. Cell backgrounds with non-default bg color generate QuadInstances. Cursor is a solid quad. No compilation errors.</done>
</task>

<task id="3" files="arcterm-render/src/text.rs" tdd="false">
  <action>
  Implement dirty-row optimization to skip re-shaping unchanged rows.

  1. Add a `row_hashes: Vec<u64>` field to TextRenderer. Each entry stores a hash of
     the row content (chars + attrs) from the last prepare.
  2. In `prepare_grid()`, for each row:
     a. Compute a hash of the row (iterate cells, hash each cell's char and attrs
        using a simple FxHash or std::hash). Use `std::collections::hash_map::DefaultHasher`.
     b. Compare with the stored hash at `row_hashes[row_idx]`.
     c. If the hash matches, skip `buf.set_rich_text()` and `buf.shape_until_scroll()`
        for that row -- the existing Buffer content is still valid.
     d. If the hash differs, re-shape the row and update the stored hash.
  3. On grid resize, clear `row_hashes` to force a full re-shape.
  4. Always re-shape the row containing the cursor (cursor may blink or move without
     cell content changing).
  5. After the grid's `mark_clean()` call (if used), the next frame should see mostly
     hash hits except for rows that actually changed.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-render && cargo test --package arcterm-render</verify>
  <done>arcterm-render builds and all tests pass. The dirty-row optimization compiles. During fast cat output, only changed rows are re-shaped each frame. The cursor row is always re-shaped.</done>
</task>
