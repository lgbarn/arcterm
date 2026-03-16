# Phase 9 — Design Decisions

## M-1: KeyInput Event Kind Fix
- **Decision:** Add a dedicated `KeyInput` variant to the WIT `EventKind` enum
- **Rationale:** Proper fix that makes the type system correct even if the event bus is used for key events in the future

## H-1: Epoch Interruption Timeout
- **Decision:** 30-second epoch deadline for WASM plugin calls
- **Rationale:** Balanced — generous enough for real plugin work, catches runaway/infinite-loop plugins within half a minute

## H-2: Plugin Tool Dispatch
- **Decision:** Full WASM function dispatch implementation (not just error cleanup)
- **Rationale:** Already touching the plugin crate for H-1, M-1, M-2, M-6 — finish the feature while in there
