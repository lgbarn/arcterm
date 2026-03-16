---
phase: phase-14
plan: "2.1"
wave: 2
dependencies: ["1.1"]
must_haves:
  - ISSUE-019 window creation graceful error (completed in PLAN-1.1, verified here)
  - No .expect() on fallible operations in arcterm-app or arcterm-render runtime paths
  - Full workspace build and test pass
files_touched:
  - arcterm-app/src/main.rs
tdd: false
---

# PLAN-2.1 — Runtime Hardening Verification

## Context

The original Phase 14 Wave 2 was scoped for M-3 (async Kitty image decode),
M-5 (GPU init returns Result), and ISSUE-019 (window creation graceful error).

Current codebase analysis shows:

- **M-3 (async image decode):** Already implemented. `terminal.rs` line 761 uses
  `tokio::task::spawn_blocking` for Kitty image decoding, with results delivered
  via an `mpsc::Sender<PendingImage>` channel. A test `async_image_decode_via_channel`
  at line 1018 verifies the pattern.

- **M-5 (GPU init returns Result):** Already implemented. `gpu.rs` line 17:
  `pub fn new(window: Arc<Window>) -> Result<Self, String>`. `Renderer::new` at
  renderer.rs line 90 propagates the Result. `main.rs` lines 1023-1030 handle
  the error with `log::error!` + `event_loop.exit()`.

- **ISSUE-019 (window creation):** The `.expect()` at main.rs:1020 is the sole
  remaining item. It is fixed in PLAN-1.1 Task 1.

This plan performs the final verification sweep: confirm no panicking paths
remain in runtime code, run the full workspace test suite, and run clippy.

## Tasks

<task id="1" files="arcterm-app/src/main.rs, arcterm-render/src/gpu.rs, arcterm-render/src/renderer.rs" tdd="false">
  <action>Audit all `.expect()` and `.unwrap()` calls in `arcterm-app/src/main.rs`, `arcterm-render/src/gpu.rs`, and `arcterm-render/src/renderer.rs` for fallible runtime paths. Specifically:

1. Verify that after PLAN-1.1 Task 1, there are zero `.expect()` calls on fallible operations in the `resumed()` function of main.rs (approximately lines 990-1230).

2. Scan `arcterm-render/src/gpu.rs` for any `.unwrap()` on fallible operations. The current code at line 54 uses `caps.formats.first().copied().unwrap_or(...)` which is safe (has a fallback). Confirm no other unwraps exist.

3. Scan `arcterm-render/src/renderer.rs` for `.expect()` or `.unwrap()` in non-test code. Document any findings.

If any new `.expect()` or `.unwrap()` calls on fallible operations are found in runtime paths (not tests, not compile-time-guaranteed operations like `"literal".parse().unwrap()`), convert them to proper error handling using the same `match` + `log::error!` + early-return pattern.
  </action>
  <verify>cargo clippy -p arcterm-app -p arcterm-render -- -D warnings 2>&1 | tail -5</verify>
  <done>Clippy passes with zero warnings for both crates. No `.expect()` calls remain on fallible operations in runtime paths of `resumed()`, `gpu.rs`, or `renderer.rs` (excluding test code and infallible operations).</done>
</task>

<task id="2" files="" tdd="false">
  <action>Run the full workspace test suite and clippy to verify Phase 14 has not introduced regressions. Execute:

1. `cargo test --workspace` — all tests must pass.
2. `cargo clippy --workspace -- -D warnings` — zero warnings.
3. `cargo build --release -p arcterm-app` — release build succeeds.

Document the test count. It should be equal to or higher than the pre-Phase-14 baseline.
  </action>
  <verify>cargo test --workspace 2>&1 | tail -20 && cargo clippy --workspace -- -D warnings 2>&1 | tail -5</verify>
  <done>All workspace tests pass. Clippy is clean across all crates. Release build succeeds. Test count is >= pre-Phase-14 baseline. Phase 14 is complete.</done>
</task>
