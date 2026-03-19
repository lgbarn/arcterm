# Research: Warp-Style AI UX

**Date**: 2026-03-19
**Feature**: 006-warp-style-ai-ux

## Decision 1: Compact Bottom Panel Rendering

**Decision**: Use the `tab_bar_at_bottom` pattern — subtract `ai_panel_height`
from `avail_height` in the resize calculation and paint synthetic `Line` objects
via `render_screen_line` in the freed space. Follow `paint_tab_bar` exactly.

**Rationale**: The tab bar already solves this problem: a fixed-height chrome
region that shrinks the terminal viewport. The rendering uses synthetic `Line`
objects painted via `render_screen_line` with no pane backing. The PTY
automatically gets fewer rows, so the shell adapts.

**Integration points**:
- `resize.rs` lines 254-263: subtract `ai_panel_height` alongside `tab_bar_height`
- `paint.rs` after `paint_tab_bar`: add `paint_ai_panel`
- New file: `render/ai_panel.rs` following `tab_bar.rs` structure
- Add `total_fixed_chrome_height()` helper to avoid updating 8 individual call sites

**Alternatives considered**:
- Split pane at bottom — adds split lifecycle complexity, user can accidentally close/zoom
- Box model render path — experimental, not production-ready

## Decision 2: `#` Prefix Interception

**Decision**: Use the AI prompt overlay shim (same mechanism as ghost text
suggestions). When at a shell prompt (OSC 133 `Input` zone), the shim's key
table intercepts Enter. On Enter: read the input zone text. If it starts with
`#`, route the query to AI and erase the line (Ctrl+U). Otherwise, send `\r`
to the shell normally.

**Critical gate**: The `#` interception MUST be gated on OSC 133 `Input` zone
detection. Without it, `#` inside vim/Python/etc. would be intercepted.

**`##` escape**: Double `#` at the start passes through as a single `#`.

**Integration points**:
- `overlay/ai_suggestion.rs`: extend existing shim with Enter interception
- Read current line via `pane.get_lines(cursor_row)` + `get_semantic_zones()` for `start_x`

## Decision 3: Step Execution Monitoring

**Decision**: Extend the OSC 133;D `CommandStatus` handler (currently a no-op)
to fire `Alert::CommandComplete { status }`. This flows through existing
`Alert → MuxNotification` infrastructure automatically. For shells without
133;D, fall back to semantic zone transition detection (Output → Input).

**Implementation**:
1. Add `last_command_exit_status: Option<i64>` to `TerminalState`
2. Add `Alert::CommandComplete { status: i64 }` to `Alert` enum
3. Fill the handler in `performer.rs` line 913 (currently empty)
4. ArcTerm AI subscriber listens for `MuxNotification::Alert::CommandComplete`
5. Fallback: track `Output → Input` zone transitions via `PaneOutput` notifications

**Rationale**: This is the only approach that provides exit codes. It adds
one field and one enum variant. All existing code paths are unchanged.
