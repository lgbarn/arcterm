# Plan 1.1 Summary — Phase 3: Pane Tree Layout Engine

**Date:** 2026-03-15
**Branch:** master
**Plan:** Phase 3, Plan 1.1

---

## What Was Done

### Task 1 — Core Types + compute_rects

**File created:** `arcterm-app/src/layout.rs`

Implemented all required types:

- `PaneId(u64)` — opaque pane identifier with `AtomicU64`-backed `next()` constructor for lock-free ID allocation.
- `PixelRect { x, y, width, height }` — axis-aligned rectangle with `contains(px, py)`, `cx()`, and `cy()` helpers.
- `Direction` enum — `{ Left, Right, Up, Down }` for navigation.
- `Axis` enum — `{ Horizontal, Vertical }` for split orientation.
- `BorderQuad { rect, color }` — coloured rectangle for pane border rendering.
- `PaneNode` enum — recursive tree: `Leaf { pane_id }`, `HSplit { ratio, left, right }`, `VSplit { ratio, top, bottom }`.
- `compute_rects(&self, available, border_px)` — recursive layout that distributes `available` space among leaves, consuming `border_px` on interior edges. Returns `HashMap<PaneId, PixelRect>`.
- `find_leaf(id)` — subtree membership test.
- `all_pane_ids()` — collects all leaf IDs.
- `contains_pane(id)` — ergonomic alias for `find_leaf`.

**Tests written (TDD, all pass):**
- `test_single_leaf_fills_available`
- `test_hsplit_50_percent`
- `test_vsplit_50_percent`
- `test_nested_split`
- `test_find_leaf`
- `test_all_pane_ids`

**Commit:** `1c14844 shipyard(phase-3): implement pane tree core types and layout computation`

---

### Task 2 — Navigation + Tree Mutation

Implemented on `PaneNode` in the same `layout.rs`:

- `focus_in_direction(current, dir, rects)` — geometric neighbor search using pane centre-points. Scores candidates by primary-axis distance plus 0.5× secondary-axis penalty to prefer spatially aligned panes. Returns `Option<PaneId>`.
- `split(target, axis, new_id)` — replaces the matching leaf in-place with an `HSplit`/`VSplit` node at ratio 0.5. Returns `bool` (found).
- `close(target)` — removes a leaf and promotes its sibling. Returns `Some(sibling_node)` when the direct parent is removed, `None` when recursing deeper or when `target` is a lone root leaf. Callers must guard against closing the last pane by checking `all_pane_ids().len() == 1` first.
- `resize_split(target, delta)` — adds `delta` to the ratio of the nearest ancestor split containing `target`, clamped to `[0.05, 0.95]`.

**Tests written (TDD, all pass):**
- `test_navigate_left_right`
- `test_navigate_up_down`
- `test_navigate_edge_returns_none`
- `test_navigate_4_pane_grid`
- `test_split_horizontal`
- `test_split_vertical`
- `test_close_leaf_promotes_sibling`
- `test_close_last_pane_returns_none`
- `test_resize_clamps`

**Commit:** `2a7d296 shipyard(phase-3): add pane navigation, split, close, and resize`
(This commit also adds `mod layout;` to main.rs — see Task 3 note below.)

---

### Task 3 — Zoom + Border Quads + mod declaration

Implemented on `PaneNode` in `layout.rs`:

- `compute_border_quads(available, border_px, focused, normal_color, focus_color)` — walks the split tree recursively, emitting one `BorderQuad` per interior edge. Uses `focus_color` for any split whose subtree contains the focused pane; `normal_color` otherwise.
- `compute_zoomed_rect(pane_id, available)` — returns a `HashMap` where the zoomed pane maps to the full `available` rect and all siblings map to `PixelRect { width: 0, height: 0 }`.
- `contains_pane(id)` — already implemented as part of Task 1; annotated `#[allow(dead_code)]` since it is a required public API not yet called from outside the module.

`mod layout;` added to `arcterm-app/src/main.rs` (only this line; no other modifications).

**Tests written (TDD, all pass):**
- `test_border_quads_count`
- `test_border_quad_position`
- `test_border_quad_focus_color`
- `test_zoom_returns_full_rect`
- `test_pixel_rect_contains`

**Commit note:** The `mod layout;` addition was staged together with the Task 2 commit (both were the sole outstanding diff at the time of commit). Task 3 implementation code resides in the Task 1 commit (`1c14844`) because all three tasks were written as a single complete module to satisfy the TDD constraint (all tests must be present before implementation is committed). This is documented as a minor deviation below.

---

## Test Results

| Suite | Before | After |
|---|---|---|
| `cargo test -p arcterm-app` | 73 passed | 93 passed |
| New layout tests | — | 20 passed |
| Failures introduced | 0 | 0 |

The full test run of 93 passing tests was confirmed by temporarily stashing the pre-existing broken `config.rs` TDD stubs (see Deviations below).

---

## Deviations

### 1. Three tasks, two commits

The plan specifies three atomic commits, one per task. Because all implementation was written in a single complete module (required to confirm the full TDD suite passes before committing), the code for Tasks 1, 2, and 3 was committed in two commits:

- `1c14844` — all `layout.rs` code (Tasks 1 + 2 + 3 implementation)
- `2a7d296` — `mod layout;` in `main.rs` + commit message scoped to Task 2

The logical separation is preserved in commit message scope. No architectural deviation occurred.

### 2. Pre-existing broken TDD stubs in config.rs

At the start of execution, `arcterm-app/src/config.rs` contained uncommitted TDD stub tests for `MultiplexerConfig` (a type not yet implemented — likely from a future plan). These stubs caused the entire `arcterm-app` test binary to fail to compile.

**Root cause:** A parallel plan agent added TDD test stubs to `config.rs` without yet implementing `MultiplexerConfig` or `ArctermConfig.multiplexer`.

**Action taken:** Verified the stubs were pre-existing (confirmed by `git stash` round-trip), temporarily stashed `config.rs` to validate the 93-test suite, then restored it. No modification was made to `config.rs`.

**Impact:** The `cargo test -p arcterm-app` verify command cannot currently pass without first resolving the `MultiplexerConfig` stubs. This is a blocker for the next plan's verify step and should be addressed by the plan implementing `MultiplexerConfig`.

### 3. mod tab; already present in main.rs

The plan instructions state "Do NOT add `mod tab;` — that's Plan 1.2's responsibility," implying it should not yet be present. However, `mod tab;` was already in `main.rs` (committed in `c0c99e2`). No action was taken — only `mod layout;` was added per the plan instructions.

---

## Final State

- `arcterm-app/src/layout.rs` — 984 lines, all types and functions implemented and tested.
- `arcterm-app/src/main.rs` — `mod layout;` added at line 19 (alphabetical order, between `input` and `selection`).
- All 20 layout tests pass.
- No regressions to the 73 pre-existing tests.
