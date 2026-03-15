---
phase: terminal-fidelity
plan: "2.3"
wave: 2
dependencies: ["1.1", "1.2"]
must_haves:
  - Mouse click/drag text selection (character, word, line)
  - Selection state tracked in app layer with cell coordinate mapping
  - Clipboard copy (Cmd+C) and paste (Cmd+V) via arboard crate
  - Selection overlay rendered as colored quads
  - Scroll viewport via mouse wheel / trackpad
  - Scroll viewport resets to bottom on new PTY output
files_touched:
  - arcterm-app/src/main.rs
  - arcterm-app/src/input.rs
  - arcterm-app/src/selection.rs (new file)
  - arcterm-app/src/terminal.rs
  - arcterm-app/Cargo.toml
tdd: true
---

# PLAN-2.3 -- Mouse Events, Text Selection, Clipboard, and Scroll Viewport

## Goal

Add mouse-driven text selection (click, double-click, triple-click, drag), clipboard
copy/paste via arboard, mouse wheel scrollback navigation, and selection overlay
rendering.

## Why Wave 2

Depends on PLAN-1.1 for `scroll_offset` and `rows_for_viewport()` (scroll viewport)
and on PLAN-1.2 for mouse reporting mode flags (to know when NOT to intercept mouse
events because the application requested mouse reporting). Does not touch arcterm-core
or arcterm-vt files. Does not overlap with PLAN-2.1 (renderer files) or PLAN-2.2
(config files) -- this plan adds new files and modifies only main.rs and input.rs.

**Rendering note:** This plan generates selection quad data but passes it to the
QuadRenderer from PLAN-2.1. If PLAN-2.1 is not yet built, the selection quads can
be stored but not rendered until the quad pipeline exists. The implementation should
define selection as `Vec<QuadInstance>` (or equivalent) that gets appended to the
quad batch in render_frame.

## Tasks

<task id="1" files="arcterm-app/src/selection.rs, arcterm-app/Cargo.toml" tdd="true">
  <action>
  Create the selection model and clipboard integration module.

  1. Add `arboard = "3"` to arcterm-app/Cargo.toml.

  2. Create `arcterm-app/src/selection.rs` with:

  3. Define `CellPos { row: usize, col: usize }` -- position in the grid (0-indexed).

  4. Define `SelectionMode` enum: `None`, `Character`, `Word`, `Line`.

  5. Define `Selection` struct:
     - `anchor: CellPos` -- where the selection started
     - `end: CellPos` -- current end of selection (follows mouse)
     - `mode: SelectionMode`
     - Methods:
       - `start(pos: CellPos, mode: SelectionMode)` -- begins a new selection
       - `update(pos: CellPos)` -- extends selection to new position
       - `normalized(&self) -> (CellPos, CellPos)` -- returns (start, end) in
         reading order (top-left to bottom-right)
       - `contains(&self, row: usize, col: usize) -> bool` -- for rendering
       - `extract_text(&self, grid: &Grid) -> String` -- extracts selected text
         from the grid, joining rows with newlines. For word/line modes, expand
         the selection boundaries appropriately.
       - `clear(&mut self)` -- resets to SelectionMode::None

  6. Define `fn pixel_to_cell(x: f64, y: f64, cell_width: f32, cell_height: f32, scale: f32) -> CellPos`
     that converts physical pixel coordinates to grid cell coordinates.

  7. Define `fn word_boundaries(row: &[Cell], col: usize) -> (usize, usize)` that
     finds the start and end column of the word at `col` (word = contiguous
     non-whitespace characters).

  8. Create a `Clipboard` struct wrapping `arboard::Clipboard`:
     - `Clipboard::new() -> Option<Clipboard>` (arboard must live for app lifetime
       per research finding #5)
     - `copy(&mut self, text: &str)` -- copies to system clipboard
     - `paste(&mut self) -> Option<String>` -- reads from system clipboard

  9. Write tests:
     - Selection normalized order when end is before anchor
     - Selection contains works for single-row and multi-row selections
     - extract_text joins multi-row selection with newlines
     - pixel_to_cell converts correctly with various scale factors
     - word_boundaries finds word edges
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test --package arcterm-app -- selection</verify>
  <done>All selection tests pass. CellPos, Selection, SelectionMode compile and work correctly. pixel_to_cell converts coordinates. word_boundaries finds word edges. extract_text produces correct multi-line text.</done>
</task>

<task id="2" files="arcterm-app/src/main.rs, arcterm-app/src/input.rs" tdd="false">
  <action>
  Wire mouse events into the application event loop for selection and scrolling.

  1. Add fields to AppState:
     - `selection: Selection`
     - `clipboard: Option<Clipboard>` (initialized once in resumed(), kept alive)
     - `last_cursor_position: winit::dpi::PhysicalPosition<f64>`
     - `last_click_time: Option<std::time::Instant>` (for double/triple-click detection)
     - `click_count: u8` (1=char, 2=word, 3=line)

  2. Add `mod selection;` to main.rs.

  3. Add WindowEvent match arms in window_event():

     `WindowEvent::CursorMoved { position, .. }`:
     - Store position in last_cursor_position.
     - If mouse button is down and selection is active, call
       selection.update(pixel_to_cell(position, cell_size, scale)).
     - Request redraw.

     `WindowEvent::MouseInput { state, button: MouseButton::Left, .. }`:
     - On press:
       - Detect double/triple click by checking elapsed time since last_click_time
         (threshold: 500ms). Increment click_count (wrap at 3).
       - Convert last_cursor_position to CellPos.
       - Start selection with mode based on click_count:
         1 => Character, 2 => Word, 3 => Line.
       - Set mouse_button_down = true.
     - On release:
       - Set mouse_button_down = false.
       - If selection has content, do NOT auto-copy (wait for explicit Cmd+C).

     `WindowEvent::MouseWheel { delta, .. }`:
     - Convert delta to line count (PixelDelta: divide by cell_height,
       LineDelta: use directly).
     - Adjust grid.scroll_offset: increase for scroll-up (review history),
       decrease for scroll-down (toward current). Clamp to 0..scrollback_len.
     - Request redraw.

  4. Add keyboard shortcut handling in the KeyboardInput match arm:
     - Cmd+C (super modifier + 'c'): if selection is active, extract text and
       clipboard.copy(). Clear selection.
     - Cmd+V (super modifier + 'v'): clipboard.paste(), then write the pasted
       text to terminal.write_input(). If bracketed_paste mode is active,
       wrap with ESC[200~ ... ESC[201~.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-app</verify>
  <done>arcterm-app builds. Mouse event handlers compile. Selection is tracked on click/drag. Cmd+C copies selected text to clipboard. Cmd+V pastes from clipboard. Mouse wheel adjusts scroll_offset.</done>
</task>

<task id="3" files="arcterm-app/src/main.rs, arcterm-app/src/terminal.rs" tdd="false">
  <action>
  Wire scroll viewport reset and selection rendering integration.

  1. In `about_to_wait()`, when new PTY data arrives (got_data = true):
     - Reset scroll_offset to 0 (snap to bottom on new output).
     - Clear any active selection (new output invalidates cell positions).

  2. In `terminal.rs`, add a method `grid_mut(&mut self) -> &mut Grid` so the app
     layer can set scroll_offset on the grid.

  3. In the render path (RedrawRequested handler), before calling render_frame():
     - If selection is active, generate QuadInstance entries for each selected cell
       (use a selection highlight color like rgba(0.3, 0.5, 0.8, 0.4)).
     - Pass these selection quads to the renderer. The exact mechanism depends on
       PLAN-2.1's QuadRenderer API -- either append to the quad batch, or pass
       separately. If PLAN-2.1 is not built yet, store the quads in a field and
       integrate when the quad pipeline is available.
     - Account for scroll_offset when mapping selection CellPos to screen row.

  4. In `terminal.rs`, add a method `set_scroll_offset(&mut self, offset: usize)`
     that sets the grid's scroll_offset field.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-app</verify>
  <done>arcterm-app builds. Scroll offset resets to 0 on new PTY output. Selection quads are generated for rendering. grid_mut and set_scroll_offset are available on Terminal. Selection is cleared when new PTY data arrives while scrolled back.</done>
</task>
