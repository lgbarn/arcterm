# Phase 13 — Context & Decisions

## Phase Summary

Fix the two known renderer performance issues: per-pane dirty-row cache and frame pacing/VSync. Add performance measurement baseline.

## Decisions

### D1: VSync / Present Mode
**Decision:** Use `PresentMode::Fifo` (VSync). Caps at display refresh rate, no tearing.
**Rationale:** Most terminals use this. Prevents idle GPU spinning. One-frame latency acceptable for terminal use.

### D2: Latency Measurement
**Decision:** Use internal timestamp logging behind the existing `latency-trace` feature flag. Log timestamps at keyboard input, PTY write, wakeup received, render submitted. Calculate deltas.
**Rationale:** Provides precise internal measurement without external tool dependency. Feature flag keeps it zero-cost in production builds.

### D3: Dirty-Row Cache Scope (from design doc)
**Decision:** Extend `TextRenderer` row hashes to `HashMap<PaneId, Vec<u64>>`. Evict on pane close, invalidate on resize. Cursor row always re-shaped.
**Rationale:** Multi-pane currently re-shapes every row every frame. This is the single biggest rendering performance issue identified in the Calyx comparison analysis.

### D4: PTY Output Coalescing
**Decision:** Coalesce rapid PTY output into single frames. Always redraw immediately on keyboard input. Only coalesce PTY-triggered redraws.
**Rationale:** Prevents `cat large_file` from queuing thousands of frames while preserving interactive typing responsiveness.
