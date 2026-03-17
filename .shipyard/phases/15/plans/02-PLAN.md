---
phase: event-handling-exit-hardening
plan: 02
wave: 2
dependencies: [01]
must_haves:
  - Layout tree is updated when panes exit (sibling promotion, focus update)
  - Multiple panes exiting simultaneously does not panic or leave stale state
  - Resize coalescing defers resize to next frame during drag
files_touched:
  - arcterm-app/src/main.rs
  - arcterm-app/src/layout.rs
tdd: false
---

# PLAN-02 — Layout Cleanup, Multi-Pane Exit, Resize Coalescing

Depends on PLAN-01 because task 1 here modifies the same pane removal loop that PLAN-01 task 2 rewrites (the `shell_exited` removal and immediate exit).

## Tasks

<task id="1" files="arcterm-app/src/main.rs" tdd="false">
  <action>
    Wire layout tree cleanup into the pane exit removal loop and handle multi-pane simultaneous exit safely.

    The current code at ~line 1662-1680 removes pane state (`state.panes.remove(id)`, image_channels, etc.) but never calls `layout.close(id)` on the layout tree, leaving stale `Leaf { pane_id }` nodes. The keyboard-driven close at lines 2968-2990 shows the correct pattern.

    Changes to the pane removal loop (inside `about_to_wait`, after `closed_panes` is populated):

    1. **Batch removal with layout update:** Replace the existing `for id in closed_panes` loop with logic that:
       - Gets `active` tab index once: `let active = state.tab_manager.active;`
       - Iterates `closed_panes`, and for each `id`:
         a. Calls `state.tab_layouts[active].close(id)` — if it returns `Some(new_root)`, replace the layout: `state.tab_layouts[active] = new_root;`
         b. Removes all associated state (panes, image_channels, auto_detectors, structured_blocks, ai_states, pane_contexts, last_ai_pane — same as current code)
         c. Sends plugin PaneClosed event (same as current code)
       - After the loop, if `state.panes.is_empty()`, call `event_loop.exit(); return;` (from PLAN-01 task 2)
       - Otherwise, update focus: if the currently focused pane was in `closed_panes`, set focus to the first remaining pane in the layout: `let remaining = state.tab_layouts[active].all_pane_ids(); if let Some(&new_focus) = remaining.first() { state.set_focused_pane(new_focus); }`

    2. **Iteration safety:** The `closed_panes` Vec is collected before any mutations (already the case at line 1444). The `for id in pane_ids` iteration that populates it uses a pre-collected `Vec<PaneId>` (line 1447), so layout/HashMap mutations during removal do not invalidate the iteration. No additional changes needed for iteration safety — the existing collect-first pattern is correct.

    3. **Multi-pane simultaneous exit:** The batch loop handles this naturally. If panes A and B both exit, both are in `closed_panes`. After removing A from the layout, B's removal still works because `PaneNode::close()` searches by `PaneId` and handles "not found" gracefully (returns `None`).
  </action>
  <verify>cargo build -p arcterm-app 2>&1 | tail -5</verify>
  <done>Pane exit removal loop calls `layout.close(id)` for each exited pane, promotes siblings, updates focus if the focused pane exited, and exits the window if all panes are gone. Build succeeds.</done>
</task>

<task id="2" files="arcterm-app/src/main.rs" tdd="false">
  <action>
    Add resize coalescing: defer the actual pane resize to `about_to_wait` instead of executing it synchronously in the `Resized` handler.

    1. **Add field to `AppState`** (~line 555 area): `pending_resize: Option<winit::dpi::PhysicalSize<u32>>,` initialized to `None`.

    2. **Modify `WindowEvent::Resized` handler** (~line 1751): Replace the current body (which calls `state.renderer.resize()`, computes rects, and resizes every pane) with:
       ```
       if size.width > 0 && size.height > 0 {
           state.pending_resize = Some(size);
           state.window.request_redraw();
       }
       ```
       This defers the work and ensures a redraw is scheduled.

    3. **Add resize execution in `about_to_wait`** (after the pane exit handling, before the `got_data` check ~line 1689): Insert:
       ```
       if let Some(size) = state.pending_resize.take() {
           state.renderer.resize(size.width, size.height);
           let rects = state.compute_pane_rects();
           let (cell_w, cell_h) = state.cell_dims();
           for (id, rect) in &rects {
               let (new_rows, new_cols) = state.grid_size_for_rect(*rect);
               if let Some(terminal) = state.panes.get_mut(id) {
                   terminal.resize(new_cols, new_rows, cell_w, cell_h);
               }
           }
           state.window.request_redraw();
       }
       ```

    This ensures that during a rapid drag-resize, only the last size is applied per frame. Multiple `Resized` events between frames overwrite `pending_resize`; only the final value is processed in `about_to_wait`.
  </action>
  <verify>cargo build -p arcterm-app 2>&1 | tail -5</verify>
  <done>`WindowEvent::Resized` sets a dirty flag instead of executing resize. `about_to_wait` applies the deferred resize once per frame. Build succeeds. During manual drag-resize testing, only one resize per frame occurs.</done>
</task>

<task id="3" files="arcterm-app/src/layout.rs" tdd="false">
  <action>
    Audit `PaneNode::close()` (line 398-451 in layout.rs) for edge cases. The method already handles:
    - Direct child leaf removal with sibling promotion (lines 406-412, 430-435)
    - Recursive search into nested splits (lines 414-427, 436-449)
    - Root-is-leaf returns None (line 400-403, caller handles)

    Verify and fix if needed:
    1. **Closing a pane not in this tab's tree:** `close()` returns `None` — safe, no mutation. No fix needed.
    2. **Closing both sides of a split simultaneously:** If pane A (left) is removed first, the root becomes the right child. Then removing pane B (now the root leaf) returns `None` from the `Leaf` arm. The caller's `all_pane_ids()` will be empty, triggering the "all panes gone" exit. Correct behavior — no fix needed.
    3. **PluginPane handling:** `pane_id_if_terminal()` at line 454 already matches both `Leaf` and `PluginPane`. Correct.

    If the audit reveals no bugs (likely based on reading), document this with a code comment at the top of `close()` noting the multi-pane exit safety property: "Safe to call multiple times in a batch — if the target is not found (already removed or in a different tab), returns None without mutation."
  </action>
  <verify>cargo build -p arcterm-app 2>&1 | tail -5</verify>
  <done>`close()` method audited for edge cases. Comment documents batch-removal safety. Build succeeds.</done>
</task>
