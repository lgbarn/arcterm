---
phase: config-runtime-hardening
plan: "1.2"
wave: 1
dependencies: []
must_haves:
  - GpuState::new() returns Result<Self, String> instead of panicking
  - Renderer::new() returns Result<Self, String>
  - All three .expect() calls in gpu.rs replaced with ? propagation
  - App::resumed() handles Err with log::error! and event_loop.exit()
  - cargo clippy clean (no unused Result warnings)
files_touched:
  - arcterm-render/src/gpu.rs
  - arcterm-render/src/renderer.rs
  - arcterm-app/src/main.rs
tdd: false
---

# PLAN-1.2 — M-5: GPU Init Safety

## Context

Three `.expect()` calls in `GpuState::new_async()` (gpu.rs lines 29, 38, 50) panic
the process on any GPU initialization failure with no user-facing error message. The
fix propagates `Result` through `GpuState::new()` -> `Renderer::new()` -> `App::resumed()`.

Error type is `String` -- consistent with existing renderer-level error handling and
avoids adding `anyhow` to `arcterm-render`. The three wgpu calls have different return
types:
- `create_surface()` returns `Result<Surface, CreateSurfaceError>` -- use `.map_err(|e| e.to_string())?`
- `request_adapter()` returns `Option<Adapter>` -- use `.ok_or_else(|| "...".to_string())?`
- `request_device()` returns `Result<(Device, Queue), RequestDeviceError>` -- use `.map_err(|e| e.to_string())?`

No runtime GPU test is feasible in CI (no display adapter). Verification is compile-time:
the Rust type system enforces that all callers handle the `Result` via `-D warnings`
(unused `Result` is a warning promoted to error by `cargo xc`).

## Tasks

<task id="1" files="arcterm-render/src/gpu.rs" tdd="false">
  <action>
  Change `GpuState::new()` signature from `-> Self` to `-> Result<Self, String>`.
  Change `new_async()` signature from `-> Self` to `-> Result<Self, String>`.

  In `new()`, wrap the `pollster::block_on` call: `pollster::block_on(Self::new_async(window))`.

  In `new_async()`, replace the three `.expect()` calls:
  1. Line 29: `.expect("failed to create wgpu surface")` becomes
     `.map_err(|e| format!("failed to create wgpu surface: {e}"))?`
  2. Line 38: `.expect("failed to find a suitable GPU adapter")` becomes
     `.ok_or_else(|| "failed to find a suitable GPU adapter".to_string())?`
  3. Line 50: `.expect("failed to create wgpu device")` becomes
     `.map_err(|e| format!("failed to create wgpu device: {e}"))?`

  Change the return from `Self { ... }` to `Ok(Self { ... })`.
  </action>
  <verify>cargo check --package arcterm-render 2>&1 | head -20</verify>
  <done>`cargo check --package arcterm-render` succeeds with no errors. `GpuState::new()` returns `Result<Self, String>`.</done>
</task>

<task id="2" files="arcterm-render/src/renderer.rs, arcterm-app/src/main.rs" tdd="false">
  <action>
  In `renderer.rs` line 84: change `Renderer::new()` signature from `-> Self` to
  `-> Result<Self, String>`. Propagate `GpuState::new(window)` with `?`. Change the
  final return from `Self { ... }` to `Ok(Self { ... })`.

  In `main.rs` line 1003: replace `let mut renderer = Renderer::new(window.clone(), cfg.font_size);`
  with a match expression:
  ```
  let mut renderer = match Renderer::new(window.clone(), cfg.font_size) {
      Ok(r) => r,
      Err(e) => {
          log::error!("GPU initialization failed: {e}");
          event_loop.exit();
          return;
      }
  };
  ```
  </action>
  <verify>cargo check --package arcterm-app 2>&1 | head -20</verify>
  <done>`cargo check --package arcterm-app` succeeds. `cargo xc` (clippy) is clean. No `.expect()` calls remain in `gpu.rs` runtime code.</done>
</task>
