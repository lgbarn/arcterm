# SUMMARY-1.2 — M-5: GPU Init Safety

## Status: Complete

## Tasks Executed

### Task 1 — GpuState::new() / new_async() Result propagation (gpu.rs)

**What changed:**
- `GpuState::new()` signature changed from `-> Self` to `-> Result<Self, String>`
- `GpuState::new_async()` signature changed from `-> Self` to `-> Result<Self, String>`
- `pollster::block_on(...)` return is now propagated directly
- Three `.expect()` calls replaced with `?` propagation:
  1. `create_surface()` — `.map_err(|e| format!("failed to create wgpu surface: {e}"))?`
  2. `request_adapter()` — `.map_err(|e| format!("failed to find a suitable GPU adapter: {e}"))?`
  3. `request_device()` — `.map_err(|e| format!("failed to create wgpu device: {e}"))?`
- `Self { ... }` return changed to `Ok(Self { ... })`

**Deviation from plan:** The plan specified `request_adapter()` returns `Option<Adapter>` and instructed use of `.ok_or_else(...)`. In this wgpu version, `request_adapter()` returns `Result<Adapter, E>`, so `.map_err(...)` was used instead. The end result is identical — errors are propagated with a descriptive message.

**Verification:** `cargo check --package arcterm-render` — PASS

**Commit:** `60c355b` — `shipyard(phase-11): propagate Result through GpuState::new() and new_async()`

---

### Task 2 — Renderer::new() and App::resumed() (renderer.rs, main.rs)

**What changed (renderer.rs):**
- `Renderer::new()` signature changed from `-> Self` to `-> Result<Self, String>`
- `GpuState::new(window)` propagated with `?`
- `Self { ... }` return changed to `Ok(Self { ... })`

**What changed (main.rs):**
- `Renderer::new(window.clone(), cfg.font_size)` wrapped in a `match` expression:
  - `Ok(r)` → assigns renderer and continues
  - `Err(e)` → logs `log::error!("GPU initialization failed: {e}")`, calls `event_loop.exit()`, returns

**Verification:** `cargo check -p arcterm-app && cargo clippy -p arcterm-app -p arcterm-render -- -D warnings` — PASS (no warnings, no errors)

**Commit:** `1500d92` — `shipyard(phase-11): propagate Result through Renderer::new(), handle in App::resumed()`

---

## Done Criteria Checklist

- [x] `GpuState::new()` returns `Result<Self, String>` instead of panicking
- [x] `Renderer::new()` returns `Result<Self, String>`
- [x] All three `.expect()` calls in `gpu.rs` runtime code replaced with `?` propagation
- [x] `App::resumed()` handles `Err` with `log::error!` and `event_loop.exit()`
- [x] `cargo clippy` clean — no unused `Result` warnings (`-D warnings` passes)
- [x] Only `gpu.rs`, `renderer.rs`, `main.rs` touched

## Files Modified

- `arcterm-render/src/gpu.rs`
- `arcterm-render/src/renderer.rs`
- `arcterm-app/src/main.rs`
