---
phase: multiplexer
plan: "1.1"
wave: 1
dependencies: []
must_haves:
  - PaneNode binary tree enum (Leaf, HSplit, VSplit) with ratio field
  - PaneId newtype with unique generation
  - PixelRect struct for layout computation
  - compute_rects() recursive layout function returning HashMap<PaneId, PixelRect>
  - Directional navigation (focus_in_direction) via upward tree traversal
  - Split and close operations that maintain tree invariants
  - Zoom toggle (bypass tree, render focused pane at full rect)
  - Minimum pane size enforcement (2 cols, 1 row)
files_touched:
  - arcterm-app/src/layout.rs
  - arcterm-app/src/main.rs (mod declaration only)
tdd: true
---

# PLAN-1.1 -- Pane Tree Layout Engine

## Goal

Implement the binary tree layout engine that computes pixel rects for panes from a tree of splits. This is the foundational data structure for the entire multiplexer: every other plan in Phase 3 depends on it. The module is pure data structures and geometry with no GPU, PTY, or windowing dependencies, making it ideal for TDD.

## Why This Must Come First

The pane tree is the central abstraction. Multi-pane rendering (Wave 2) needs `compute_rects()` to know where to draw each pane. The keymap (Wave 2) needs `focus_in_direction()` to navigate. Tab model (Wave 1, parallel) wraps a `PaneNode`. Without this module, nothing else in Phase 3 can proceed.

## Tasks

<task id="1" files="arcterm-app/src/layout.rs" tdd="true">
  <action>Create `arcterm-app/src/layout.rs` with:

1. `PaneId` -- a `#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug)] struct PaneId(u64)` with a `PaneId::next()` constructor using `AtomicU64`.

2. `PixelRect` -- `struct PixelRect { pub x: f32, pub y: f32, pub width: f32, pub height: f32 }` with a `contains(px, py) -> bool` method.

3. `Direction` -- `enum Direction { Left, Right, Up, Down }`.

4. `PaneNode` enum:
   ```
   enum PaneNode {
       Leaf { pane_id: PaneId },
       HSplit { ratio: f32, left: Box<PaneNode>, right: Box<PaneNode> },
       VSplit { ratio: f32, top: Box<PaneNode>, bottom: Box<PaneNode> },
   }
   ```

5. `PaneNode::compute_rects(&self, available: PixelRect, border_px: f32) -> HashMap<PaneId, PixelRect>` -- recursive layout. For HSplit: left gets `width * ratio - border_px/2`, right gets the remainder minus `border_px/2`. For VSplit: top gets `height * ratio - border_px/2`, bottom gets the remainder. Enforce minimum pane dimensions: if a child rect would be smaller than `MIN_PANE_WIDTH` (16.0 px) or `MIN_PANE_HEIGHT` (16.0 px), clamp to the minimum and reduce the sibling.

6. `PaneNode::find_leaf(&self, id: PaneId) -> bool` -- returns true if the pane exists in the tree.

7. `PaneNode::all_pane_ids(&self) -> Vec<PaneId>` -- collect all leaf IDs in tree order.

Write tests first covering:
- Single leaf returns one rect equal to the available rect
- HSplit at 0.5 returns two rects each roughly half the width (minus border)
- VSplit at 0.5 returns two rects each roughly half the height (minus border)
- Nested split (HSplit containing a VSplit child) computes 3 rects correctly
- `find_leaf` returns true for existing IDs, false for unknown IDs
- `all_pane_ids` returns all leaf IDs in order</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app layout -- --nocapture</verify>
  <done>All layout unit tests pass. `PaneNode::compute_rects` correctly subdivides rects for single, two-pane, and nested splits with border spacing.</done>
</task>

<task id="2" files="arcterm-app/src/layout.rs" tdd="true">
  <action>Add directional navigation and tree mutation methods to `PaneNode`:

1. `PaneNode::focus_in_direction(current: PaneId, dir: Direction, rects: &HashMap<PaneId, PixelRect>) -> Option<PaneId>` -- given the current focused pane and computed rects, find the nearest pane in the specified direction. Algorithm: get the current pane's rect center point. Filter all other pane rects to those that are strictly in the target direction (e.g., for `Left`, candidates must have `rect.x + rect.width <= current_rect.x`). Among candidates, pick the one whose center is closest (Euclidean distance) to the current center. Return `None` if no candidate exists (edge of layout).

2. `PaneNode::split(target: PaneId, axis: Axis, new_id: PaneId) -> bool` -- find the leaf with `target` and replace it with an HSplit or VSplit (ratio 0.5) where the original leaf is the first child and a new `Leaf { pane_id: new_id }` is the second child. Return `true` if the target was found and split. `Axis` is `enum Axis { Horizontal, Vertical }`.

3. `PaneNode::close(target: PaneId) -> Option<PaneNode>` -- remove the leaf with `target` and promote its sibling to take the parent split's place. If the tree is just a single leaf with `target`, return `None` (cannot close the last pane). If the target is not found, return `Some(self)` unchanged.

4. `PaneNode::resize_split(target: PaneId, delta: f32) -> bool` -- find the parent split node of the target leaf and adjust its ratio by `delta` (clamped to 0.1..0.9). Return true if found.

Write tests first covering:
- Navigation left from a right pane in an HSplit returns the left pane
- Navigation right from the left pane returns the right pane
- Navigation up/down from a VSplit works correctly
- Navigation at the edge returns None
- Navigation in a 4-pane grid (HSplit of two VSplits) traverses correctly
- Split a single leaf creates an HSplit with two children
- Close one pane in an HSplit promotes the sibling to root
- Close the last pane returns None
- resize_split adjusts ratio and clamps to bounds</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app layout -- --nocapture</verify>
  <done>All navigation, split, close, and resize tests pass. `focus_in_direction` correctly traverses geometric neighbors in all 4 directions including nested layouts.</done>
</task>

<task id="3" files="arcterm-app/src/layout.rs, arcterm-app/src/main.rs" tdd="true">
  <action>Add zoom support and border quad generation, plus wire the module declaration:

1. `PaneNode::compute_border_quads(&self, available: PixelRect, border_px: f32, focused: PaneId, focus_color: [f32; 4], unfocus_color: [f32; 4]) -> Vec<BorderQuad>` where `BorderQuad` is `struct BorderQuad { pub rect: PixelRect, pub color: [f32; 4] }`. Recursively walk the tree: at each split node, emit one border quad (1px line at the split boundary). If either child contains the focused pane, use `focus_color`; otherwise use `unfocus_color`. HSplit: vertical border line at `x + width * ratio`, full height, width = `border_px`. VSplit: horizontal border line at `y + height * ratio`, full width, height = `border_px`.

2. `compute_zoomed_rect(pane_id: PaneId, available: PixelRect) -> HashMap<PaneId, PixelRect>` -- returns a single-entry map with the given pane_id mapped to the full available rect. Used when zoom is toggled.

3. `PaneNode::contains_pane(&self, id: PaneId) -> bool` -- helper used by border color logic.

4. In `arcterm-app/src/main.rs`, add `mod layout;` to the module declarations (after existing mod lines). Do NOT add any other changes to main.rs in this task.

Write tests covering:
- Single leaf produces zero border quads
- HSplit produces one vertical border quad at the correct position
- VSplit produces one horizontal border quad at the correct position
- Nested 3-pane layout produces 2 border quads
- Border adjacent to focused pane uses focus_color, others use unfocus_color
- `compute_zoomed_rect` returns exactly one entry with the full available rect</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-app layout -- --nocapture && cargo clippy -p arcterm-app -- -D warnings</verify>
  <done>All border quad and zoom tests pass. Clippy reports no warnings. The `layout` module is declared in `main.rs` and compiles cleanly.</done>
</task>
