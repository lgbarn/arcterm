---
phase: multiplexer
plan: "3.3"
wave: 3
dependencies: ["1.1", "1.2", "2.1", "2.2"]
must_haves:
  - Command palette overlay (Ctrl+Space opens, Escape closes)
  - Fuzzy substring search over command list
  - Arrow up/down selection, Enter executes
  - Palette renders as semi-transparent overlay via quad + text pipeline
  - Commands for all Phase 3 pane/tab operations
files_touched:
  - arcterm-app/src/palette.rs
  - arcterm-app/src/main.rs (mod declaration + palette input routing)
tdd: true
---

# PLAN-3.3 -- Command Palette

## Goal

Implement the command palette overlay that provides fuzzy-searchable access to all pane and tab management commands. This is a modal UI that intercepts all input when open and renders as a floating overlay on top of the terminal content.

## Why Wave 3

Depends on the keymap (2.2) for `OpenPalette` action, and on the rendering pipeline (2.1) for overlay quad support. The palette produces `KeyAction`-equivalent commands that are dispatched through the same machinery wired in PLAN-3.1.

## Tasks

<task id="1" files="arcterm-app/src/palette.rs" tdd="true">
  <action>Create `arcterm-app/src/palette.rs` with the palette state machine and command registry:

1. `PaletteCommand` struct:
   ```rust
   pub struct PaletteCommand {
       pub label: &'static str,
       pub description: &'static str,
       pub action: PaletteAction,
   }
   ```

2. `PaletteAction` enum (mirrors KeyAction for command dispatch):
   ```rust
   pub enum PaletteAction {
       SplitHorizontal,
       SplitVertical,
       ClosePane,
       ToggleZoom,
       NewTab,
       CloseTab,
       NavigateLeft,
       NavigateRight,
       NavigateUp,
       NavigateDown,
   }
   ```

3. `fn default_commands() -> Vec<PaletteCommand>` -- returns the fixed list of Phase 3 commands:
   - "Split Horizontal" / "Split pane horizontally" / SplitHorizontal
   - "Split Vertical" / "Split pane vertically" / SplitVertical
   - "Close Pane" / "Close the focused pane" / ClosePane
   - "Toggle Zoom" / "Zoom/unzoom the focused pane" / ToggleZoom
   - "New Tab" / "Create a new tab" / NewTab
   - "Close Tab" / "Close the current tab" / CloseTab
   - "Navigate Left/Right/Up/Down" (4 entries)

4. `PaletteState` struct:
   ```rust
   pub struct PaletteState {
       pub query: String,
       pub commands: Vec<PaletteCommand>,
       pub filtered: Vec<usize>,  // indices into commands
       pub selected: usize,       // index into filtered
   }
   ```

5. `PaletteState::new() -> Self` -- creates with empty query and all commands visible.

6. `PaletteState::update_filter(&mut self)` -- recomputes `filtered` using case-insensitive substring match of `query` against `label`. Resets `selected` to 0.

7. `PaletteState::handle_input(&mut self, event: &KeyEvent) -> PaletteInput`:
   ```rust
   pub enum PaletteInput {
       Consumed,         // key handled, palette stays open
       Close,            // Escape pressed, close palette
       Execute(PaletteAction), // Enter pressed, execute selected command
   }
   ```
   - Printable characters: append to `query`, call `update_filter`, return `Consumed`.
   - Backspace: remove last char from `query`, call `update_filter`, return `Consumed`.
   - ArrowUp: decrement `selected` (saturating), return `Consumed`.
   - ArrowDown: increment `selected` (capped at `filtered.len() - 1`), return `Consumed`.
   - Enter: if `filtered` is non-empty, return `Execute(commands[filtered[selected]].action)`.
   - Escape: return `Close`.

Write tests:
- New palette has all commands visible
- Typing "split" filters to "Split Horizontal" and "Split Vertical"
- Typing "zoom" filters to "Toggle Zoom"
- Empty query shows all commands
- ArrowDown increments selected
- ArrowDown at end stays at end
- Backspace removes last char and re-filters
- Enter returns Execute with correct action
- Escape returns Close</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app palette -- --nocapture</verify>
  <done>All palette state machine tests pass. Fuzzy filtering, selection, and input handling produce correct results.</done>
</task>

<task id="2" files="arcterm-app/src/palette.rs" tdd="false">
  <action>Add rendering data generation to `PaletteState`:

1. `PaletteState::render_quads(&self, window_width: f32, window_height: f32, cell_w: f32, cell_h: f32, scale: f32) -> Vec<PaletteQuad>` where `PaletteQuad` has the same shape as `OverlayQuad`:
   - Full-screen dimming quad: `[0, 0, window_width, window_height]`, color `[0.0, 0.0, 0.0, 0.5]`.
   - Palette box: centered horizontally, 60% of window width, positioned at 20% from top. Height = `(filtered.len().min(10) + 2) * cell_h * scale`. Background color: `[0.15, 0.15, 0.2, 0.95]`.
   - Input field background: top row of the palette box, slightly lighter `[0.2, 0.2, 0.25, 1.0]`.
   - Selected row highlight: the row at `selected` index, color `[0.3, 0.3, 0.4, 1.0]`.

2. `PaletteState::render_text_content(&self) -> Vec<(String, f32, f32)>` -- returns `(text, x_offset, y_offset)` tuples for each visible element:
   - Input field: `"> {query}"` at the top of the palette box.
   - Each filtered command label at its corresponding row position.
   - Maximum 10 visible results (scroll if more).

These return plain data structures -- the actual TextArea/QuadInstance construction happens in the render integration (PLAN-3.1's render path, or a follow-up task).</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-app && cargo clippy -p arcterm-app -- -D warnings 2>&1 | tail -5</verify>
  <done>Palette rendering data generation compiles and passes clippy. Quad positions and text content are correctly computed for the overlay layout.</done>
</task>

<task id="3" files="arcterm-app/src/main.rs" tdd="false">
  <action>Integrate the command palette into `AppState`:

1. Add `mod palette;` to `main.rs`.
2. Add `palette_mode: Option<palette::PaletteState>` to `AppState` (initialized to `None`).
3. In `WindowEvent::KeyboardInput`:
   - If `palette_mode.is_some()`: call `palette.handle_input(event)`.
     - On `PaletteInput::Close`: set `palette_mode = None`, request redraw.
     - On `PaletteInput::Execute(action)`: convert `PaletteAction` to the equivalent `KeyAction` and dispatch through the existing KeyAction handler. Set `palette_mode = None`.
     - On `PaletteInput::Consumed`: request redraw (palette content changed).
   - This check happens BEFORE the keymap handler (palette captures all input when open).
4. In the `KeyAction::OpenPalette` dispatch: set `palette_mode = Some(PaletteState::new())`, request redraw.
5. In `WindowEvent::RedrawRequested`: if `palette_mode.is_some()`, append palette quads to the overlay_quads list passed to `render_multipane`. For palette text, add TextAreas to the text area collection using the palette's render_text_content positions.

Verify manually: press Ctrl+Space to open palette, type to filter, arrow keys to select, Enter to execute, Escape to close.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-app && cargo clippy -p arcterm-app -- -D warnings 2>&1 | tail -5</verify>
  <done>`cargo build` and `cargo clippy` succeed. Command palette opens on Ctrl+Space, renders as an overlay, filters commands, and dispatches the selected action on Enter.</done>
</task>
