# Phase 14 — Context & Decisions

## Phase Summary

Fix surviving issues from v0.1.1 review that affect `arcterm-app`, `arcterm-render`, and `arcterm-plugin`. These were not eliminated by the Phase 12 engine swap.

## Decisions

### D1: H-2 (WASM tool dispatch) scope
**Decision:** Dispatch-only. Implement basic WASM function call and return. No retry, timeout, or error recovery.
**Rationale:** Smallest scope that satisfies the requirement. Full implementation deferred to v0.3.0.

### D2: Parallelism strategy
**Decision:** Two parallel waves. Wave 1: app/input fixes + plugin fixes in parallel (different crates). Wave 2: runtime hardening (touches both).
**Rationale:** App and plugin crates have no file overlap. Parallel execution cuts build time.

## Issue Groups

### Wave 1A — App/Input Fixes (arcterm-app)
- ISSUE-002: Missing request_redraw() after keyboard input
- ISSUE-003: Ctrl+\ and Ctrl+] not handled
- ISSUE-004: PTY creation failure panics
- ISSUE-005: Shell exit "Shell exited" indicator
- ISSUE-006: Cursor invisible on blank cells

### Wave 1B — Plugin Fixes (arcterm-plugin)
- H-1: Epoch-increment background task
- H-2: Real WASM tool dispatch (dispatch-only)
- M-1: KeyInput event kind mapping
- M-2: Plugin manifest path traversal validation
- M-6: Plugin file copy rejects symlinks
- ISSUE-015: Backslash validation test
- ISSUE-016: Epoch ticker thread cleanup
- ISSUE-017: Double-lock TOCTOU in call_tool
- ISSUE-018: Canonicalize fallback

### Wave 2 — Runtime Hardening (arcterm-app + arcterm-render)
- M-3: Async Kitty image decode
- M-5: GPU init returns Result
- ISSUE-019: Window creation graceful error
