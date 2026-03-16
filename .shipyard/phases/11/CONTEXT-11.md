# Phase 11 — Design Decisions

## M-3: Async Kitty Image Decode
- **Decision:** Use `tokio::task::spawn_blocking` with mpsc channel
- **Rationale:** Decode images on tokio's blocking thread pool, send decoded results via mpsc channel, drain completed decodes in render loop before each frame. Fully async, no PTY thread blocking.

## M-4: Scrollback Lines Cap
- **Decision:** Cap at 1,000,000 lines
- **Rationale:** Generous enough for any real workflow (~400MB worst case at 80 cols). Only triggers on clearly unreasonable values. Log a warning when clamped.

## M-5: GPU Init Safety
- **Decision:** `GpuState::new()` returns `Result` instead of panicking
- **Rationale:** Propagate error through `Renderer::new()` → `AppState` init. Display user-facing error message on GPU init failure.
