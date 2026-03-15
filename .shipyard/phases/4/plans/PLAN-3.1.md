---
phase: structured-output
plan: "3.1"
wave: 3
dependencies: ["2.1", "2.2"]
must_haves:
  - StructuredBlock overlay rendering in render_multipane (colored text areas at pane-relative positions)
  - Terminal upgraded from Grid to GridState handler (required for accumulator)
  - ApcScanner wrapping Processor in Terminal for Kitty graphics readiness
  - Auto-detector wired into PTY output processing
  - OSC 7770 completed blocks consumed and rendered
  - Copy button quad on code blocks
  - Non-protocol tools render identically to Phase 2
files_touched:
  - arcterm-app/src/terminal.rs
  - arcterm-app/src/main.rs
  - arcterm-render/src/renderer.rs
  - arcterm-render/src/text.rs
  - arcterm-render/src/structured.rs
tdd: false
---

# PLAN-3.1 -- Structured Block Rendering Integration

## Goal

Wire together the OSC 7770 parser, auto-detection engine, content renderers, and the GPU rendering pipeline so that structured blocks are visible as colored overlays on top of the terminal grid. This is the end-to-end integration plan that makes rich content actually appear on screen.

## Why Wave 3

This plan depends on every Wave 1 and Wave 2 deliverable: the OSC 7770 parser (PLAN-1.1), the APC scanner (PLAN-1.2), the content renderers (PLAN-2.1), and the auto-detection engine (PLAN-2.2). It wires them together and extends the render pipeline.

## Design Notes

**Terminal handler upgrade**: `Terminal` currently uses bare `Grid` as the handler. It must be upgraded to `GridState` (which wraps `Grid` and adds scroll region, saved cursor, and now the structured content accumulator). Additionally, `Processor` must be replaced with `ApcScanner` (which wraps `Processor` internally).

**Rendering approach**: Structured blocks are rendered as additional `TextArea` entries in the existing `submit_text_areas` pipeline. For each `StructuredBlock`, the renderer:
1. Computes the pixel rect from `(block.start_row, 0)` using `cell_size`.
2. Draws a background quad covering the block's row range (slightly tinted to distinguish from normal output).
3. Prepares a rich-text glyphon Buffer using the block's `RenderedLine` spans.
4. Adds it to `pane_buffer_pool` as a new slot.

**Copy button**: A small quad (16x16 px) rendered at the top-right corner of code blocks. Hit-testing is handled in the input layer using pixel coordinates.

**Critical constraint**: Non-protocol output MUST render identically to Phase 2. The overlay is additive -- it draws ON TOP of the grid text. When no structured blocks exist, the render path is unchanged.

## Tasks

<task id="1" files="arcterm-app/src/terminal.rs, arcterm-app/src/main.rs" tdd="false">
  <action>Upgrade Terminal from Grid to GridState handler, and wrap Processor with ApcScanner:

1. In `arcterm-app/src/terminal.rs`:
   - Change `grid: Grid` field to `grid_state: GridState`.
   - Change `processor: Processor` to `scanner: ApcScanner`.
   - Import `GridState` from `arcterm_vt` and `ApcScanner` from `arcterm_vt`.
   - Update `new()`: create `GridState::new(Grid::new(size))` and `ApcScanner::new()`.
   - Update `process_pty_output()`: call `self.scanner.advance(&mut self.grid_state, bytes)`.
   - Update `grid()`: return `&self.grid_state.grid`.
   - Update `grid_mut()`: return `&mut self.grid_state.grid`.
   - Update `resize()`: resize `self.grid_state.grid` (and also update scroll_bottom on grid_state).
   - Update `take_pending_replies()`: drain from `self.grid_state.grid.pending_replies`.
   - Add `pub fn take_completed_blocks(&mut self) -> Vec<StructuredContentAccumulator>`: drain `self.grid_state.completed_blocks`.
   - Add `pub fn grid_state(&self) -> &GridState` accessor.

2. In `arcterm-app/src/main.rs`:
   - Update all call sites that access `terminal.grid()` -- these should continue to work since `grid()` still returns `&Grid`.
   - The cursor modes (cursor_visible, app_cursor_keys) are now on `terminal.grid_state().modes` instead of `terminal.grid().modes`. Update references in the render and input handlers.

3. Verify the app still compiles and runs with `cargo run -p arcterm-app`. A basic `ls` command should render identically to before.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-app 2>&1 | tail -5</verify>
  <done>`cargo build` succeeds. Terminal uses GridState and ApcScanner. All existing references to `terminal.grid()` compile. TermModes accessed via `grid_state().modes`.</done>
</task>

<task id="2" files="arcterm-render/src/renderer.rs, arcterm-render/src/text.rs, arcterm-render/src/structured.rs" tdd="false">
  <action>Extend the rendering pipeline to overlay structured blocks:

1. In `arcterm-render/src/renderer.rs`:
   - Add a `structured_blocks` field to `PaneRenderInfo`: `pub structured_blocks: &'a [StructuredBlock]`.
   - In `render_multipane`, after `prepare_grid_at(pane.grid, ...)`, iterate `pane.structured_blocks`:
     - For each block, compute the pixel y-origin: `py + block.start_row as f32 * cell_h * sf`.
     - Compute block pixel height: `block.rendered_lines.len() as f32 * cell_h * sf`.
     - Add a background tint quad (dark blue/gray, e.g., [0.12, 0.14, 0.18, 0.92]) covering the block rect to `all_quads`.
     - Call a new `self.text.prepare_structured_block(...)` method to shape the block's rich text.
   - For code blocks, add a copy button quad (small white square, 14x14 px) at the top-right corner of the block rect.

2. In `arcterm-render/src/text.rs`:
   - Add `pub fn prepare_structured_block(&mut self, block: &StructuredBlock, offset_x: f32, offset_y: f32, clip: Option<ClipRect>, scale_factor: f32)`:
     - For each `RenderedLine` in `block.rendered_lines`, create a glyphon Buffer.
     - Use `buf.set_rich_text(...)` with spans mapped from `StyledSpan` to glyphon `Attrs::new().family(Family::Monospace).color(Color::rgb(span.color.0, span.color.1, span.color.2))`, with `.weight(Weight::BOLD)` if span.bold and `.style(Style::Italic)` if span.italic.
     - Add each buffer to the `pane_buffer_pool` as a new pane slot, positioned at the correct pixel offset within the block.

3. In `arcterm-render/src/structured.rs`:
   - Add `pub use` exports for `StructuredBlock`, `RenderedLine`, `StyledSpan`, `HighlightEngine` from `lib.rs`.

4. In `arcterm-render/src/lib.rs`:
   - Re-export key structured types: `pub use structured::{HighlightEngine, StructuredBlock, RenderedLine, StyledSpan};`.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-render 2>&1 | tail -5</verify>
  <done>`cargo build` succeeds. `PaneRenderInfo` accepts `structured_blocks`. `prepare_structured_block` shapes rich text and adds to pane buffer pool. Background tint quads and copy button quads render for structured blocks.</done>
</task>

<task id="3" files="arcterm-app/src/main.rs" tdd="false">
  <action>Wire the full structured output pipeline in the app event loop:

1. Add `HighlightEngine` to `AppState` (initialized once in `resumed()`).

2. Add `AutoDetector` per pane (stored alongside each Terminal in the tab/pane data structure).

3. Add `Vec<StructuredBlock>` per pane for rendered blocks.

4. In the PTY output processing path (where `process_pty_output` is called):
   - After processing PTY output, call `terminal.take_completed_blocks()` to get any OSC 7770 blocks.
   - For each completed block, call `highlight_engine.render_block(block.content_type, &block.buffer, &block.attrs)` to produce `Vec<RenderedLine>`.
   - Create a `StructuredBlock` with the rendered lines, the start_row (cursor row at block start), and the line count.
   - Append to the pane's `Vec<StructuredBlock>`.

5. Also in the PTY output processing path:
   - Call `auto_detector.scan_rows(grid.cells_ref(), cursor_row)` on the grid rows.
   - For each `DetectionResult`, call `highlight_engine.render_block(...)` and create a `StructuredBlock`.
   - Append to the pane's `Vec<StructuredBlock>`.

6. In the render path (where `PaneRenderInfo` is constructed):
   - Pass `structured_blocks: &pane_blocks[..]` in `PaneRenderInfo`.

7. Add copy button click handling:
   - In the mouse click handler, check if the click position falls within any code block's copy button rect.
   - If so, copy `block.raw_content` to the system clipboard using the existing `arboard` integration.

8. Test manually:
   - Run `echo -e '\x1b]7770;start;type=code_block;lang=rust\x07fn main() {\n    println!("hello");\n}\x1b]7770;end\x07'` and verify syntax-highlighted output.
   - Run `git diff` in a repo and verify auto-detected diff coloring.
   - Run `ls -la` and verify NO structured rendering (zero interference).
   - Run `cat some.json` with a valid JSON file and verify auto-detected JSON formatting.

Note: The exact OSC 7770 escape format uses ST (ESC \) as the terminator, not BEL. Adjust the test echo commands accordingly: `\x1b]7770;start;type=code_block;lang=rust\x1b\\`.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-app 2>&1 | tail -5 && cargo clippy -p arcterm-app -- -D warnings 2>&1 | tail -5</verify>
  <done>Full pipeline compiles and clips clean. OSC 7770 blocks are parsed, highlighted, and rendered as overlays. Auto-detected content is highlighted. Copy button renders on code blocks. Standard shell output (`ls`, `top`, `vim`) renders identically to Phase 2.</done>
</task>
