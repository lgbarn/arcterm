---
phase: multiplexer
plan: "3.1"
wave: 3
dependencies: ["1.1", "1.2", "2.1", "2.2"]
must_haves:
  - AppState restructured from single Terminal to TabManager + HashMap<PaneId, Terminal> + HashMap<PaneId, Receiver>
  - about_to_wait polls all pane PTY channels (all tabs, not just active)
  - Window resize recomputes all pane rects and resizes all pane Terminals
  - KeyboardInput routes through KeymapHandler, dispatches KeyActions
  - Split creates new Terminal + PTY, inserts into tree
  - Close pane shuts down PTY, removes from tree
  - Tab switching renders only the active tab
  - Zoom toggle renders focused pane fullscreen
  - Pane resize adjusts split ratios
  - render_multipane called with computed pane rects, borders, and tab bar
  - Selection scoped to focused pane only
  - Mouse scroll scoped to focused pane only
files_touched:
  - arcterm-app/src/main.rs
  - arcterm-app/src/terminal.rs
tdd: false
---

# PLAN-3.1 -- AppState Restructuring and Full Integration

## Goal

Rewire `AppState` in `main.rs` from the single-pane architecture to the full multiplexer. This is the largest and most critical plan in Phase 3 -- it connects every module built in Waves 1-2 into a working system. After this plan, arcterm supports multiple panes with splits, tabs, navigation, and zoom.

## Why Wave 3

Every previous plan is a prerequisite: layout engine (1.1), tab model (1.2), multi-pane rendering (2.1), and keymap (2.2). This plan integrates them all.

## Tasks

<task id="1" files="arcterm-app/src/terminal.rs, arcterm-app/src/main.rs" tdd="false">
  <action>Restructure `AppState` to support multiple panes and tabs. This is a large refactor of `main.rs`:

**Terminal changes** (`terminal.rs`):
- Add `pub fn shutdown(&mut self)` method that calls `self.pty.shutdown()` (for clean pane close).
- Make `Terminal::new` return type unchanged -- each pane creates its own Terminal.

**AppState changes** (`main.rs`):
- Remove: `terminal: Terminal`, `pty_rx: mpsc::Receiver<Vec<u8>>`.
- Add:
  ```rust
  tab_manager: tab::TabManager,
  panes: HashMap<PaneId, Terminal>,
  pty_channels: HashMap<PaneId, mpsc::Receiver<Vec<u8>>>,
  keymap: keymap::KeymapHandler,
  ```
- In `resumed()`:
  1. Create the first `PaneId` via `PaneId::next()`.
  2. Create `Terminal::new(grid_size, shell)` as before, getting `(terminal, pty_rx)`.
  3. Create `TabManager::new(first_pane_id)`.
  4. Insert terminal into `self.panes` and pty_rx into `self.pty_channels`.
  5. Create `KeymapHandler::new(config.multiplexer.leader_timeout_ms)`.

**about_to_wait changes**:
- Poll ALL pane PTY channels (iterate `self.pty_channels`), not just one. For each channel that yields data, call the corresponding Terminal's `process_pty_output`. Track whether the active tab had data (for redraw request).
- Drain pending replies for ALL panes (not just the active one).
- Window title: use the focused pane's grid title.

**Resize changes**:
- On `WindowEvent::Resized`: recompute all pane rects via `tab_manager.active_tab().layout.compute_rects(available_rect, border_px)`. For each pane in the active tab, call `terminal.resize(grid_size_from_rect)`. Background tab panes are resized lazily when switched to.

**RedrawRequested changes**:
- Compute pane rects from the active tab's layout tree (or zoomed rect if zoom is active).
- Build `Vec<PaneRenderInfo>` from pane rects + grids.
- Build border overlay quads from `compute_border_quads`.
- Build tab bar quads from `render_tab_bar_quads`.
- Call `renderer.render_multipane(panes, overlay_quads, scale_factor)`.
- Shell-exited banner: show only if the focused pane's shell has exited.

**Selection and scroll**: scope to the focused pane only. `cursor_to_cell` subtracts the focused pane's rect origin from the mouse position.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-app 2>&1 | tail -10</verify>
  <done>`cargo build` succeeds. The application starts with a single pane in a single tab, rendering through the multi-pane pipeline. Visual output is identical to Phase 2.</done>
</task>

<task id="2" files="arcterm-app/src/main.rs" tdd="false">
  <action>Wire all `KeyAction` dispatches into the `WindowEvent::KeyboardInput` handler:

Replace the existing keyboard input handling with:
1. If `palette_mode.is_some()`, route to palette handler (stub for now -- just handle Escape to close).
2. Otherwise, call `self.keymap.handle_key(event, modifiers, app_cursor_keys)`.
3. Match on the returned `KeyAction`:
   - `Forward(bytes)`: write to the focused pane's Terminal via `self.panes.get_mut(&focus).unwrap().write_input(&bytes)`.
   - `NavigatePane(dir)`: call `PaneNode::focus_in_direction(current_focus, dir, &rects)`. If Some(new_id), update `tab_manager.active_tab_mut().focus = new_id`. Request redraw (border colors change).
   - `Split(axis)`: create new `PaneId`, create new `Terminal::new(...)`. Insert into `self.panes` and `self.pty_channels`. Call `tab_manager.active_tab_mut().layout.split(focus, axis, new_pane_id)`. Recompute rects and resize all affected panes. Request redraw.
   - `ClosePane`: call `tab_manager.active_tab_mut().layout.close(focus)`. If returns None (last pane), close the tab instead. Otherwise, remove Terminal and pty_channel for the closed pane, call `terminal.shutdown()`. Set focus to the promoted sibling. Request redraw.
   - `ToggleZoom`: toggle `tab_manager.active_tab_mut().zoomed`. If currently None, set to `Some(focus)`. If currently Some, set to None. Request redraw.
   - `ResizePane(dir)`: call `tab_manager.active_tab_mut().layout.resize_split(focus, delta)` where delta is +0.05 for Right/Down and -0.05 for Left/Up. Recompute rects and resize affected panes. Request redraw.
   - `NewTab`: create new PaneId and Terminal. Call `tab_manager.add_tab(new_id)`. Insert into panes/channels. Switch to the new tab. Request redraw.
   - `SwitchTab(idx)`: call `tab_manager.switch_to(idx)`. Resize panes in the newly active tab (they may not have been resized since last active). Request redraw.
   - `CloseTab`: get pane IDs from `tab_manager.close_tab(active)`. For each, shutdown Terminal and remove from maps. Request redraw.
   - `OpenPalette`: set `palette_mode = Some(PaletteState::new())` (stub).
   - `Consumed`: do nothing.

4. Keep the Cmd+C / Cmd+V handling (copy/paste) before the keymap dispatch, as these are OS-level shortcuts that bypass the leader system.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-app 2>&1 | tail -10</verify>
  <done>`cargo build` succeeds. All KeyAction variants are dispatched. Split creates a new pane visible on screen. Close removes it. Tab switching works. Pane navigation via Ctrl+h/j/k/l moves focus and updates border colors.</done>
</task>

<task id="3" files="arcterm-app/src/main.rs" tdd="false">
  <action>Wire mouse interactions for pane focus and tab switching:

1. **Click-to-focus**: In `WindowEvent::MouseInput` (left button pressed), check if the click position falls within a non-focused pane's rect. If so, update `tab_manager.active_tab_mut().focus` to that pane's ID. Compute pane rects and test each rect with `PixelRect::contains(px, py)`.

2. **Tab bar click**: If the click y-coordinate is within the tab bar height, compute which tab label was clicked based on x-coordinate and tab widths. Call `tab_manager.switch_to(clicked_tab_index)`.

3. **Mouse scroll scoping**: In `WindowEvent::MouseWheel`, determine which pane the cursor is over (using `PixelRect::contains` on all active pane rects). Apply the scroll offset change to that pane's grid, not necessarily the focused pane.

4. **Selection scoping**: In `WindowEvent::CursorMoved`, only extend selection if the cursor is within the focused pane's rect. Offset the pixel-to-cell conversion by the focused pane's rect origin.

5. **Pane border drag resize**: In `WindowEvent::CursorMoved`, detect if the cursor is within 3px of a border line. If so, change cursor icon to resize. On `MouseInput` press near a border, start a drag resize. On `CursorMoved` during drag, compute the new ratio from the mouse position relative to the parent split's total extent. Call `layout.resize_split()`. On release, end the drag.

Store drag state as `resize_drag: Option<ResizeDrag>` on AppState where `ResizeDrag { pane_id: PaneId, start_pos: f64 }`.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-app && cargo clippy -p arcterm-app -- -D warnings 2>&1 | tail -10</verify>
  <done>`cargo build` and `cargo clippy` succeed. Click-to-focus, tab bar clicks, scoped mouse scroll, scoped selection, and border drag resize all compile. Manual verification: clicking a pane focuses it, clicking tab bar switches tabs, dragging a border resizes panes.</done>
</task>
