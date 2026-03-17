# v0.2.1 — Event Handling & Exit Hardening

## Motivation

Phase 12-14 built a working terminal on alacritty_terminal, but manual testing revealed two runtime bugs: the window didn't close on `exit`, and CPU was spinning when idle. Both were hotfixed, but the investigation uncovered several remaining gaps in event handling, pane lifecycle, and exit flows that need systematic hardening before v0.2.0 ships.

## Scope

One phase (Phase 15) covering six areas:

### 1. Reader Thread EOF Wakeup (REVIEW-2.1-H)
Clone `wakeup_tx` into the reader thread. Send a final wakeup before breaking on EOF. Currently mitigated by `has_exited()` but the edge case exists for shells that close the PTY fd without triggering ChildExit.

### 2. Remove Exit Banner — Auto-Close Immediately
The current code sets `shell_exited = true`, renders a banner, then auto-closes on the next `about_to_wait`. The banner is never visible to the user. Remove the banner rendering code entirely. When the last pane exits, `event_loop.exit()` should be called in the same `about_to_wait` iteration — no redraw, no banner, no extra cycle.

### 3. Layout Tree Cleanup on Pane Close
When a pane exits, `state.panes.remove(id)` removes the terminal but the `PaneNode` tree may retain a stale `Leaf { pane_id }`. Audit and fix:
- Call `close_pane(id)` on the layout tree to trigger sibling promotion
- If the closed pane was in a split, the sibling should take the full space
- If the closed pane was the last in a tab, remove the tab
- Update focused pane ID if the closed pane was focused

### 4. Multi-Pane Simultaneous Exit
When multiple panes exit in the same `about_to_wait` cycle:
- All exited panes should be collected first, then removed in batch
- Layout tree mutations should not invalidate the iteration
- If all panes exit simultaneously, `shell_exited` should be set and window closed in the same cycle

### 5. Resize Coalescing
During window drag-resize, `Resized` events fire rapidly. Each triggers full per-pane resize (Term resize + TIOCSWINSZ + re-render). Add coalescing: on `Resized`, set a dirty flag and defer the actual resize to `about_to_wait` or the next frame. This prevents dozens of resize+render cycles during a drag.

### 6. Fifo Fallback Logging
Log the actual `PresentMode` selected at startup. If Fifo is unavailable on the platform, log a warning so the user knows frame pacing may be degraded.

## What Stays the Same

- `ControlFlow::Poll` when active / `Wait` when idle (confirmed correct by research)
- `PresentMode::Fifo` (confirmed correct)
- Keyboard input path (working, all issues resolved)
- PTY write path (working, blocking send)
- Wakeup channel (unbounded, drain pattern correct)

## Success Criteria

- `exit` in single-pane closes window immediately (no banner, no extra frame)
- Closing one side of a split promotes the sibling to full size
- Closing the last pane in a multi-pane layout closes the window
- Multiple panes exiting simultaneously doesn't panic or leave stale state
- Window drag-resize doesn't cause visible lag or dozens of intermediate renders
- `PresentMode` is logged at startup
- Reader thread EOF sends wakeup
- All existing tests pass
- No regressions in keyboard input latency
