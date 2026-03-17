# Phase 15 — Context & Decisions

## Phase Summary

Harden event handling, pane lifecycle, and exit flows. Final gate before v0.2.0.

## Decisions

### D1: Exit behavior
**Decision:** Auto-close immediately when last pane exits. No banner, no key press required.
**Rationale:** User explicitly chose option A. Clean and simple.

### D2: Multi-pane scope
**Decision:** Include multi-pane exit robustness — layout tree cleanup, sibling promotion, simultaneous exit.
**Rationale:** Pane exit touches the same code paths. Fixing together avoids half-broken flows.

### D3: Resize coalescing
**Decision:** Defer resize to next frame during drag. Set dirty flag on Resized event, apply in about_to_wait.

### D4: Frame pacing approach
**Decision:** Keep current Poll (active) / Wait (idle) + Fifo. Confirmed correct by research against Alacritty/Rio/WezTerm.
