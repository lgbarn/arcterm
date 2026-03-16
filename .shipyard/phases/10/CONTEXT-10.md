# Phase 10 — Design Decisions

## ISSUE-006: Cursor Visibility on Blank Cells
- **Decision:** Render cursor as block character glyph (U+2588) when on blank/space cells
- **Rationale:** Simple fix within existing text pipeline. Defer dedicated wgpu quad pass to v0.2.0.

## Phase 9 API Migration
- **Note:** Phase 9 made `scroll_offset` private in arcterm-core. Phase 10 must update `arcterm-app/src/main.rs` to use `set_scroll_offset()` / `scroll_offset()` accessor methods.
