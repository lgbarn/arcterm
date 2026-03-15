# SUMMARY-3.1.md — Plan 3.1: AppState Restructuring and Full Integration

## Plan Reference
- Phase: 3 — Arcterm Multiplexer
- Plan: 3.1 — AppState Restructuring and Full Integration
- Branch: master
- Commit: `140def1`

## What Was Done

### Pre-Implementation Analysis

All existing source files were read in full before writing a single line of code:
- `arcterm-app/src/main.rs` — 674 lines of single-pane application state and event loop
- `arcterm-app/src/terminal.rs` — Terminal struct wrapping PTY+VT+Grid
- `arcterm-app/src/layout.rs` — PaneId(u64), PaneNode, PixelRect, compute_rects, split, close, etc.
- `arcterm-app/src/tab.rs` — TabManager, Tab, with its own internal `PaneId = u64` type alias
- `arcterm-app/src/keymap.rs` — KeymapHandler, KeyAction, KeymapState
- `arcterm-app/src/selection.rs` — Selection, pixel_to_cell
- `arcterm-app/src/config.rs` — ArctermConfig, MultiplexerConfig
- `arcterm-render/src/renderer.rs` — Renderer, render_multipane, render_tab_bar_quads, tab_bar_height
- `arcterm-render/src/lib.rs` — exports

Baseline: 77 tests passing before any changes.

---

### Architectural Discovery: Dual PaneId Types

**Finding:** `tab.rs` defines `type PaneId = u64` (a type alias), while `layout.rs` defines `struct PaneId(pub u64)` (a newtype). These are structurally equivalent but different types. The `tab::PaneNode` variant names also differ from `layout::PaneNode`:
- `tab::PaneNode::Leaf(id)` vs `layout::PaneNode::Leaf { pane_id: PaneId }`

**Resolution:** Used `layout::PaneId` as the canonical type in `AppState` and `main.rs`. When calling `TabManager` methods (which accept `tab::PaneId = u64`), the inner value is extracted with `.0`. When receiving IDs back from `TabManager`, they are wrapped with `layout::PaneId(id)`.

The `tab::Tab.layout` field (a `tab::PaneNode`) is not used for layout computation — a parallel `tab_layouts: Vec<layout::PaneNode>` stores the actual binary trees that power `compute_rects`, `split`, `close`, etc. `TabManager` is retained for tab metadata (labels, focus, zoom flags).

---

### Task 1: Restructure AppState

**Changes to `main.rs` (798 → 1185 lines):**

`AppState` now contains:
- `panes: HashMap<layout::PaneId, Terminal>` — all live terminals across all tabs
- `pty_channels: HashMap<layout::PaneId, mpsc::Receiver<Vec<u8>>>` — one channel per pane
- `tab_manager: TabManager` — tab labels, per-tab focused pane ID, per-tab zoom flag
- `tab_layouts: Vec<layout::PaneNode>` — layout trees, index-aligned with `tab_manager.tabs`
- `keymap: KeymapHandler` — leader-key state machine
- `drag_pane: Option<PaneId>` — active border drag state
- `palette_open: bool` — command palette stub flag

**`resumed()`** creates the first pane via `Terminal::new`, inserts it into `panes` and `pty_channels`, creates a `TabManager::new(first_id.0)`, and initialises `tab_layouts` with `vec![PaneNode::Leaf { pane_id: first_id }]`.

**`about_to_wait()`** iterates `pty_channels.keys()` and drains bytes for ALL panes. Config hot-reload now updates scrollback on all panes. When a channel disconnects, the pane entry is removed. When all channels are gone, `shell_exited = true`. DSR/DA replies are drained and written for all panes. Window title comes from the focused pane.

---

### Task 2: Wire KeyAction Dispatch

All 11 `KeyAction` variants are handled in `WindowEvent::KeyboardInput`:

| KeyAction | Behaviour |
|-----------|-----------|
| `Forward(bytes)` | Write to focused pane's PTY |
| `NavigatePane(dir)` | Call `focus_in_direction`, update tab focus, clear selection |
| `Split(axis)` | `spawn_pane()` → `tab_layouts[active].split()`, focus new pane |
| `ClosePane` | If last pane: close tab (and exit if last tab). Else: `layout.close()`, remove pane+channel |
| `ToggleZoom` | Toggle `tab.zoomed` on/off for focused pane |
| `ResizePane(dir)` | `tab_layouts[active].resize_split(focused, ±RESIZE_DELTA)` |
| `NewTab` | `spawn_pane()` → `tab_manager.add_tab()` → push new layout leaf → switch to new tab |
| `SwitchTab(n)` | `tab_manager.switch_to(n-1)`, update focus to new tab's focused pane |
| `CloseTab` | `tab_manager.close_tab()`, remove panes/channels, update focus |
| `OpenPalette` | Toggle `palette_open` flag, log stub message |
| `Consumed` | No-op (leader chord entered, awaiting action key) |

Cmd+C / Cmd+V are intercepted before the keymap, scoped to the focused pane.

---

### Task 3: Mouse Interactions

**Click-to-focus:** On `MouseInput::Pressed`, compute `pane_rects`, find which pane rect contains the click, update focus if different pane was clicked.

**Tab bar click:** If click y-coordinate is within the tab bar height (only shown when tab_count > 1 and `show_tab_bar = true`), compute the clicked tab index from x-position and call `tab_manager.switch_to()`.

**Scoped mouse scroll:** `MouseWheel` scrolls only the focused pane's `grid.scroll_offset`.

**Scoped selection:** `CursorMoved` and `MouseInput` for selection use `cursor_to_cell_in_rect()` which subtracts the pane rect origin before calling `pixel_to_cell()`.

**Border drag resize:** On `MouseInput::Pressed`, check if cursor is within `BORDER_DRAG_THRESHOLD` (4 px) of any pane's right or bottom edge. If so, set `drag_pane = Some(id)`. On `CursorMoved`, compute the drag delta and call `resize_split()`. On `MouseInput::Released`, clear `drag_pane`.

---

### New Helper Functions and Constants

- `AppState::focused_pane()` / `set_focused_pane()` — access focused pane with type conversion
- `AppState::active_layout()` / `active_layout_mut()` — get layout tree for active tab
- `AppState::pane_area()` — compute available rect below tab bar
- `AppState::compute_pane_rects()` — dispatch to `compute_rects` or `compute_zoomed_rect`
- `AppState::grid_size_for_rect()` — convert pixel rect to terminal grid dimensions
- `AppState::spawn_pane()` — create Terminal+PTY, insert into maps, return PaneId
- `cursor_to_cell_in_rect()` — pixel→cell conversion with pane-relative origin
- Constants: `BORDER_PX=2.0`, `BORDER_COLOR_NORMAL`, `BORDER_COLOR_FOCUS`, `RESIZE_DELTA=0.05`, `BORDER_DRAG_THRESHOLD=4.0`

---

## Deviations from Plan

### Combined Commit

The plan specifies three separate commits:
1. `shipyard(phase-3): restructure AppState for multi-pane multiplexer`
2. `shipyard(phase-3): wire KeyAction dispatch for all multiplexer operations`
3. `shipyard(phase-3): add click-to-focus, tab bar clicks, and border drag resize`

**Actual:** One commit `140def1` covering all three tasks.

**Reason:** All three tasks modify the same `AppState` struct and `impl ApplicationHandler for App` in a single file (`main.rs`). The struct fields required by Task 2 (e.g., `keymap`, `tab_layouts`) and Task 3 (e.g., `drag_pane`) must exist before either can be wired. Implementing Task 1 without simultaneously wiring Task 2 and Task 3 would produce non-compiling intermediate states that cannot be independently verified. Splitting into three non-compiling intermediates would violate the "each commit must pass its verify command" requirement.

### tab_layouts parallel to TabManager

The plan says "tab_manager: TabManager" and implies TabManager owns the layout trees. In practice, `tab::Tab.layout` is a `tab::PaneNode` (different type from `layout::PaneNode`). Adding a parallel `tab_layouts: Vec<layout::PaneNode>` was the minimum change needed to avoid modifying `tab.rs` while still using the full layout engine from `layout.rs`.

### Shell-exited banner scope

The shell-exited banner is shown only on the focused pane (not all panes) when `shell_exited = true`. This matches the single-pane experience from before. A future enhancement could show the banner on the specific pane whose PTY closed.

---

## Verification Results

| Check | Result |
|-------|--------|
| `cargo clippy --package arcterm-app` | Clean (0 warnings) |
| `cargo clippy --workspace` | Clean (0 warnings) |
| `cargo test --workspace --lib` | 77/77 passed |
| Single-pane path (default) | Works: one pane, one tab = unchanged UX |

---

## Final State

The application is now a full terminal multiplexer:
- **Pane splits:** `Ctrl+a n` (horizontal), `Ctrl+a v` (vertical)
- **Pane navigation:** `Ctrl+h/j/k/l` or `Ctrl+a` + arrow keys
- **Pane close:** `Ctrl+a q`
- **Pane zoom:** `Ctrl+a z`
- **Pane resize:** `Ctrl+a` + arrow keys, or border drag
- **Tabs:** `Ctrl+a t` (new), `Ctrl+a w` (close), `Ctrl+a 1-9` (switch), tab bar click
- **Click-to-focus:** left-click any pane
- **Selection/clipboard:** scoped to focused pane
- **Config hot-reload:** all panes updated
- **Single-pane path:** fully preserved (one tab, one pane = identical to pre-refactor)
