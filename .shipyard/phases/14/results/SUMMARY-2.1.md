# SUMMARY-2.1 — Runtime Hardening Verification

**Plan:** PLAN-2.1
**Phase:** 14
**Wave:** 2
**Date:** 2026-03-16
**Status:** COMPLETE — no code changes required

---

## Task 1: Audit `.expect()` and `.unwrap()` in Runtime Code

### Files Audited

- `arcterm-app/src/main.rs`
- `arcterm-app/src/context.rs`
- `arcterm-app/src/terminal.rs`
- `arcterm-app/src/workspace.rs`
- `arcterm-app/src/config.rs`
- `arcterm-app/src/overlay.rs`
- `arcterm-app/src/neovim.rs`
- `arcterm-app/src/keymap.rs`
- `arcterm-app/src/layout.rs`
- `arcterm-app/src/input.rs`
- `arcterm-app/src/colors.rs`
- `arcterm-app/src/plan.rs`
- `arcterm-render/src/gpu.rs`
- `arcterm-render/src/renderer.rs`
- `arcterm-render/src/structured.rs`

### Findings

**arcterm-render/src/gpu.rs:**
- Line 54: `caps.formats.first().copied().unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb)` — uses `unwrap_or`, safe fallback, not a panic.
- No `.expect()` calls found anywhere in the file.
- No `.unwrap()` panicking calls found.

**arcterm-render/src/renderer.rs:**
- No `.expect()` or `.unwrap()` calls found in any runtime path.
- Frame errors handled via full `match` on `self.gpu.begin_frame()` with `log::error!` and `log::warn!` + `return`.

**arcterm-app/src/main.rs `resumed()` function (lines 999–end):**
- Zero `.expect()` calls — ISSUE-019 fix confirmed (window creation uses `match` + `log::error!` + `event_loop.exit()` at line 1017).
- GPU initialization uses `match` + `log::error!` + `event_loop.exit()` at line 1026.
- Four `.unwrap()` calls remain, all infallible:
  - **Line 1554:** `exit_codes.last().unwrap()` — guarded by preceding `!exit_codes.is_empty()` check.
  - **Line 2515:** `overlay_review.take().unwrap()` — guarded by preceding `overlay_review.is_some()` check.
  - **Line 2614:** `search_overlay.take().unwrap()` — guarded by preceding `search_overlay.is_some()` check.
  - **Line 2779:** `nvim_states.get(&focused_id).unwrap()` — key inserted on line 2775 in the same block, unconditionally present.

**arcterm-app/src/main.rs `main()` function (lines 517–533):**
- Three `.unwrap()` calls in startup/process-bootstrap paths:
  - **Line 520:** `tokio::runtime::Builder::new_multi_thread().build().unwrap()` — process cannot continue without a Tokio runtime; no recovery possible.
  - **Line 523:** `EventLoop::new().unwrap()` — process cannot run without an event loop; no recovery possible.
  - **Line 532:** `event_loop.run_app(&mut app).unwrap()` — top-level event loop exit; standard winit pattern.
- These are not hot-path panics and represent the accepted Rust pattern for unrecoverable startup failures.

**arcterm-app/src/keymap.rs:**
- **Line 245:** `d.parse::<usize>().unwrap()` — the match arm pattern constrains `d` to `"1"` through `"9"`, so parsing is compile-time-guaranteed infallible.

**All other `.expect()` and `.unwrap()` occurrences** in `context.rs`, `terminal.rs`, `workspace.rs`, `config.rs`, `overlay.rs`, `neovim.rs`, `input.rs`, `colors.rs`, `plan.rs`, `layout.rs`, and `arcterm-render/src/structured.rs` are exclusively in `#[test]` and `#[tokio::test]` annotated functions.

### Conclusion

**Zero panicking paths remain in runtime code.** No changes were needed. All `.expect()` and `.unwrap()` in non-test code are either:
1. Startup-path failures where no recovery is architecturally possible, or
2. Logically infallible operations guarded by preceding checks or constrained by pattern matching.

### Verify Result

```
$ cargo clippy -p arcterm-app -p arcterm-render -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.90s
```

Zero warnings. Clippy clean.

---

## Task 2: Full Workspace Verification

### Test Results

| Crate / Suite | Passed | Failed | Ignored |
|---|---|---|---|
| arcterm-app (suite 1) | 21 | 0 | 0 |
| arcterm-app (suite 2) | 322 | 0 | 0 |
| arcterm-app (suite 3) | 3 | 0 | 3 |
| arcterm-app (suite 4) | 25 | 0 | 0 |
| arcterm-app (suite 5) | 4 | 0 | 0 |
| arcterm-render | 41 | 0 | 0 |
| doc-tests (arcterm-app) | 0 | 0 | 0 |
| doc-tests (arcterm-plugin) | 0 | 0 | 0 |
| doc-tests (arcterm-render) | 0 | 0 | 0 |
| **Total** | **416** | **0** | **3** |

All 416 tests pass. 3 tests ignored (GPU/display-dependent tests that require a physical display).

### Clippy Result

```
$ cargo clippy --workspace -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.24s
```

Zero warnings across all workspace crates.

### Release Build Result

```
$ cargo build --release -p arcterm-app
Finished `release` profile [optimized] target(s) in 1m 06s
```

Release binary builds successfully.

---

## M-3 and M-5 Status (Pre-verified)

- **M-3 (async Kitty image decode):** Implemented in Phase 12. `terminal.rs` uses `tokio::task::spawn_blocking` + `mpsc::Sender<PendingImage>`. Test `async_image_decode_via_channel` at line 1018 passes.
- **M-5 (GPU init returns Result):** Implemented in Phase 12. `GpuState::new` at `gpu.rs:17` returns `Result<Self, String>`. `Renderer::new` propagates it. `main.rs` handles it with `match` + `log::error!` + `event_loop.exit()`.

---

## Deviations

None. The audit confirmed the codebase was already clean. No code changes were made.

---

## Phase 14 State

Phase 14 is complete. All must-have items are satisfied:
- ISSUE-019 (window creation graceful error): Fixed in PLAN-1.1.
- No `.expect()` on fallible operations in runtime paths: Confirmed.
- Full workspace build and test pass: Confirmed (416 tests, 0 failures).
