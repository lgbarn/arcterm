---
plan: 02
wave: 2
reviewer: claude-sonnet-4-6
status: approved
---

# REVIEW-02 — Layout Cleanup, Multi-Pane Exit, Resize Coalescing

## Stage 1: Spec Compliance

**Verdict:** PASS

---

### Task 1: Wire layout.close() into pane exit loop + focus update

- **Status:** PASS
- **Evidence:**
  - `/Users/lgbarn/Personal/arcterm/arcterm-app/src/main.rs:1665` — `let active = state.tab_manager.active;` is captured once before the loop, exactly as specified.
  - `/Users/lgbarn/Personal/arcterm/arcterm-app/src/main.rs:1666–1688` — the `for id in closed_panes` loop calls `state.tab_layouts[active].close(id)` for each pane (line 1668), and when `Some(new_root)` is returned, replaces the layout entry (line 1669). All associated state (`panes`, `image_channels`, `auto_detectors`, `structured_blocks`, `ai_states`, `pane_contexts`, `last_ai_pane`) is removed in the same loop iteration. `PaneClosed` plugin event is sent.
  - `/Users/lgbarn/Personal/arcterm/arcterm-app/src/main.rs:1690–1695` — `panes.is_empty()` triggers `event_loop.exit(); return;` (the PLAN-01 all-panes-gone path remains intact and fires after the removal loop).
  - `/Users/lgbarn/Personal/arcterm/arcterm-app/src/main.rs:1697–1707` — after the exit guard, the focus update block calls `state.focused_pane()`, checks `panes.contains_key`, and if the focused pane is gone calls `tab_layouts[active].all_pane_ids()` then `set_focused_pane(remaining.first())`. This matches the spec exactly.
  - Iteration safety: `closed_panes` is a pre-collected `Vec<PaneId>` assembled at line 1444; the removal loop mutates layout and HashMaps but does not invalidate this Vec.
  - Multi-tab invariant: `tab_layouts.remove()` only happens inside keyboard-driven `window_event` handlers, never in `about_to_wait`, so `active` cannot be stale by the time the removal loop runs.
- **Notes:** The spec listed a specific state field sequence for removal (panes, image_channels, auto_detectors, structured_blocks, ai_states, pane_contexts, last_ai_pane). The implementation matches this list completely with no omissions.

---

### Task 2: Resize coalescing — pending_resize field + deferred apply

- **Status:** PASS
- **Evidence:**
  - `/Users/lgbarn/Personal/arcterm/arcterm-app/src/main.rs:594` — `pending_resize: Option<winit::dpi::PhysicalSize<u32>>` declared in `AppState` struct with a three-line doc comment explaining the coalescing intent.
  - `/Users/lgbarn/Personal/arcterm/arcterm-app/src/main.rs:1242` — initialized to `None` in the constructor.
  - `/Users/lgbarn/Personal/arcterm/arcterm-app/src/main.rs:1787–1795` — `WindowEvent::Resized` handler now contains only the guard `if size.width > 0 && size.height > 0` followed by `state.pending_resize = Some(size); state.window.request_redraw();`. The previous immediate `renderer.resize` + `compute_pane_rects` + per-pane `terminal.resize` body is fully removed.
  - `/Users/lgbarn/Personal/arcterm/arcterm-app/src/main.rs:1709–1723` — `about_to_wait` drains the pending size with `.take()` and executes `renderer.resize`, `compute_pane_rects`, `cell_dims`, per-pane `grid_size_for_rect` + `terminal.resize`, and `window.request_redraw()`. Placement is after pane exit handling (including focus update) and before the `if got_data` branch at line 1725, matching the spec placement requirement.
  - The plan's pseudocode body matches the implementation verbatim.
- **Notes:** No deviations. The SUMMARY accurately described the placement as "top of the `got_data` section" — it is just before the `if got_data` block, which is consistent with the spec's "before the `got_data` check" wording.

---

### Task 3: PaneNode::close() audit + doc comment

- **Status:** PASS
- **Evidence:**
  - `/Users/lgbarn/Personal/arcterm/arcterm-app/src/layout.rs:399–409` — expanded doc comment added above `pub fn close()`. It documents: (a) the batch-removal safety invariant ("if the target is not present… returns `None` without mutating any state"), (b) the two-pane split simultaneous-exit example with `HSplit(A, B)`, showing that after A's removal the root becomes `Leaf(B)`, a subsequent `close(B)` hits the `Leaf` arm and returns `None`, and the caller's `panes.is_empty()` triggers exit.
  - The existing implementation at lines 411–463 was not modified — audit confirmed no bugs.
  - All three audit cases from the spec (not-found, both-sides-of-split, PluginPane) are addressed in the comment.
- **Notes:** The spec said the comment should cover "if the target is not found (already removed or in a different tab), returns None without mutation." The actual comment says "already removed, belongs to a different tab, or the root is a lone leaf" — correctly broadening to include the lone-leaf case, which is the third `None` scenario.

---

### No-regression check

- **Status:** PASS
- **Evidence:**
  - `cargo test --workspace`: 416 tests passed across all crates (21 + 322 + 6 + 25 + 4 + 41), 0 failed.
  - `cargo clippy --workspace -- -D warnings`: clean, no warnings.

---

## Stage 2: Code Quality

### Critical

None.

### Important

- **Focus update uses `all_pane_ids()` on the active tab only, but multi-tab pane exit paths are not exercised** — `/Users/lgbarn/Personal/arcterm/arcterm-app/src/main.rs:1665,1702`
  - The `active` index is captured once from `state.tab_manager.active` before the removal loop. If a pane exits in a background tab (a tab other than the currently viewed tab), `tab_layouts[active]` is the wrong layout tree. `close(id)` will return `None` (pane not found in active tab's tree), leaving the stale `Leaf` node in the background tab's layout. The subsequent focus update reads `tab_layouts[active].all_pane_ids()` on the active tab, which is unaffected, so focus remains coherent for the visible tab — but the background tab's tree is now permanently stale.
  - The spec does not explicitly address background-tab exit handling, but the task's "batch removal works" done criterion implies correctness across all tabs. In a single-tab app this is moot; with multiple tabs it is a real gap.
  - Remediation: Iterate over all closed panes and match each `pane_id` against `tab_layouts` by searching all entries, not just `tab_layouts[active]`. A helper `fn tab_index_for_pane(&self, id: PaneId) -> Option<usize>` that calls `all_pane_ids()` on each tab's layout would allow routing `close(id)` to the correct tab.

### Suggestions

- **Doc comment on `close()` example uses `panes.is_empty()` but the actual check is `state.panes.is_empty()`** — `/Users/lgbarn/Personal/arcterm/arcterm-app/src/layout.rs:408`
  - The comment says "The caller detects `panes.is_empty()` and exits." `PaneNode::close()` is a method on the layout tree; it has no visibility into `AppState::panes`. The comment is accurate in intent but could mislead a reader of `layout.rs` into thinking the layout type itself exposes `panes`. Wording such as "The application-level `panes` HashMap is now empty, so the caller exits" makes the ownership boundary clearer.
  - Remediation: Change the comment to "The caller's pane-state map is now empty (`state.panes.is_empty()`); the application exits. No panic, no stale nodes."

- **`pending_resize` applies resize after focus update rather than before** — `/Users/lgbarn/Personal/arcterm/arcterm-app/src/main.rs:1709`
  - The ordering pane-exit → focus-update → resize is generally safe, but if `set_focused_pane` triggers any immediate layout recomputation in the future, the resize has not yet propagated. Currently `set_focused_pane` only updates a focus field (line 679–683), so this is a non-issue today. Noting for awareness.
  - Remediation: No action required now. If `set_focused_pane` gains side effects, the resize block should precede or follow the focus update with care.

---

## Summary

**Verdict:** APPROVE

All three tasks are implemented exactly as specified. `layout.close()` is correctly wired into the pane exit loop with sibling promotion, focus is updated when the focused pane exits, and the `pending_resize` coalescing pattern matches the plan's pseudocode verbatim. Tests pass and clippy is clean. One Important finding is logged for the background-tab pane exit gap — the active-tab assumption in the `active` capture is benign in the current single-tab-at-a-time usage pattern but will become a stale-layout bug if background-tab exits are ever possible.

Critical: 0 | Important: 1 | Suggestions: 2
