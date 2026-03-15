---
plan: "2.3"
reviewer: claude-sonnet-4-6
date: 2026-03-15
verdict: REQUEST CHANGES
stage1: FAIL
---

# Review 2.3 -- GPU Window and Text Rendering

## Stage 1: Spec Compliance
**Verdict: FAIL**

Three deviations prevent a Stage 1 pass: (1) `begin_frame` signature does not match the spec and removes the caller's ability to recover from `SurfaceError::Lost`; (2) `TextRenderer::render` signature diverges from the spec; and (3) the `about_to_wait` handler is absent from the window example, violating an explicit spec requirement.

---

### Task 1: GpuState (`arcterm-render/src/gpu.rs`)
- Status: FAIL
- Evidence: `/Users/lgbarn/Personal/myterm/arcterm-render/src/gpu.rs:88`

The spec mandates:

```
GpuState::begin_frame(&self) -> Result<(wgpu::SurfaceTexture, wgpu::TextureView), wgpu::SurfaceError>
```

The done criteria explicitly states: "Caller handles `SurfaceError::Lost` by calling resize."

The implementation signature is:

```rust
pub fn begin_frame(&self) -> (wgpu::SurfaceTexture, wgpu::TextureView)
```

The fallible `get_current_texture()` call is handled internally with `.expect("failed to acquire next swapchain texture")` (line 92), which panics on `SurfaceError::Lost` instead of returning it to the caller. The caller in `renderer.rs` can therefore never detect surface loss and call `resize()` to recover. This is a functional correctness deviation: on a surface-lost event (common on macOS during window occlusion/restoration) the process panics rather than recovering gracefully.

All other GpuState requirements are correctly implemented: struct fields match spec, initialization sequence is correct (backends::all, HighPerformance, downlevel_webgl2_defaults with adapter resolution, Fifo present mode), resize guards against zero dimensions, and `cargo check` passes.

---

### Task 2: TextRenderer (`arcterm-render/src/text.rs`)
- Status: FAIL
- Evidence: `/Users/lgbarn/Personal/myterm/arcterm-render/src/text.rs:152-157`

The spec mandates:

```
TextRenderer::render(&self, atlas: &glyphon::TextAtlas, viewport: &glyphon::Viewport, render_pass: &mut wgpu::RenderPass)
```

The spec also mandates: "Then call `self.atlas.trim()`" -- i.e., trim happens inside `render()`.

The implementation signature is:

```rust
pub fn render<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) -> Result<(), glyphon::RenderError>
```

Atlas and viewport are not passed as arguments; they are accessed via `self`. Additionally, atlas trimming was split into a separate `trim_atlas()` method that callers must invoke manually after frame submission (`renderer.rs:89`), rather than being called inside `render()` as specified.

The atlas-and-viewport-as-arguments signature in the spec is clearly intended to make the method's data dependencies explicit and to decouple the renderer from its owned resources during borrowing. While the implemented approach compiles and works, it deviates from the specified interface. The split `trim_atlas()` is a functional deviation: if a caller forgets to call it (as would be easy to do), the atlas grows unboundedly. `render_frame` in `renderer.rs` does call it, but the spec's intent was to make this automatic inside `render()`.

Note: The done criteria references "glyphon 0.9 API" but the implementation correctly uses glyphon 0.10 (matching the workspace dependency). This is a stale version reference in the plan, not a builder error.

All other TextRenderer requirements pass: struct fields are present, `prepare_grid` uses per-row buffers with per-cell color spans via `set_rich_text`, full 256-color palette is implemented (ANSI 16 hardcoded, 216-color cube computed, greyscale ramp 232-255), `ansi_color_to_glyphon` handles all three `Color` variants, `cargo check` passes.

---

### Task 3: Renderer + Window Example (`arcterm-render/src/renderer.rs`, `arcterm-render/examples/window.rs`)
- Status: FAIL
- Evidence: `/Users/lgbarn/Personal/myterm/arcterm-render/examples/window.rs` -- `about_to_wait` handler absent

The spec requires:

```
In `about_to_wait`: call `window.request_redraw()`.
```

This handler is entirely absent. The only `request_redraw()` call in the example is inside the `Resized` branch (line 94). Without `about_to_wait` triggering continuous redraws, the window renders one frame on startup and then remains static until a resize event occurs. This means `RedrawRequested` is only fired reactively, not on every event loop tick -- the window does not continuously render as required for a functional terminal renderer smoke test.

Additional minor deviations (informational, not failing on their own):

- Window size: spec says 800x600, implementation uses 1024x768.
- Row 3 color: spec says "Bold blue" with `fg=Indexed(4)`, implementation uses "Cyan text row" with `fg=Indexed(14)`. The bold attribute is also absent from any row (the spec required bold=true on row 3).
- Row 4 in spec is not defined; implementation adds an RGB orange row and a 256-color cube strip -- this is an additive improvement beyond spec.

`Renderer` struct, `new()`, `resize()`, `render_frame()`, `grid_size_for_window()`, and re-exports in `lib.rs` all match the spec in structure. `cargo build --package arcterm-render --example window` passes (0.24s, already cached).

---

## Stage 2: Code Quality
Stage 2 is not performed because Stage 1 failed. Fix the three failures above and resubmit.

---

## Issues to Track

The following items should be appended to `.shipyard/ISSUES.md` once the file is created.

### Critical (blocking)

**C1 -- `begin_frame` panics on `SurfaceError::Lost` instead of propagating**
- File: `/Users/lgbarn/Personal/myterm/arcterm-render/src/gpu.rs:88-96`
- The function must return `Result<(wgpu::SurfaceTexture, wgpu::TextureView), wgpu::SurfaceError>`. Replace the `.expect()` call with `?` propagation. The caller in `renderer.rs::render_frame` must then match on `Err(wgpu::SurfaceError::Lost)` and call `self.resize(w, h)` before retrying.

**C2 -- `about_to_wait` handler missing from window example**
- File: `/Users/lgbarn/Personal/myterm/arcterm-render/examples/window.rs`
- Add `fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop)` to the `ApplicationHandler` impl. Inside, if `self.state.is_some()`, call `self.state.as_ref().unwrap().window.request_redraw()`. Without this, the render loop is not continuous.

### Important (non-blocking)

**I1 -- `TextRenderer::render` signature deviates from spec; `trim_atlas` not called inside `render`**
- File: `/Users/lgbarn/Personal/myterm/arcterm-render/src/text.rs:152-162`
- The spec signature passes `atlas` and `viewport` as explicit arguments. While the current approach works, it ties the render method's borrowing to self. More critically, atlas trimming should be called inside `render()` to prevent callers from forgetting it. Either move `self.atlas.trim()` into `render()`, or document the `trim_atlas()` contract prominently in the public API with a `# Panics` / `# Note` doc comment explaining the required call order.

**I2 -- `render_frame` silently swallows prepare and render errors**
- File: `/Users/lgbarn/Personal/myterm/arcterm-render/src/renderer.rs:55, 84`
- Both `prepare_grid` and `render` return `Result` but are called with `let _ = ...`. At minimum, log the errors via `log::warn!`. For `render`, an error (e.g., `RenderError::AtlasFull`) should trigger `self.text.trim_atlas()` and a retry or graceful degradation, not silent discard.

**I3 -- Example row 3 missing bold attribute; color does not match spec**
- File: `/Users/lgbarn/Personal/myterm/arcterm-render/examples/window.rs:133-139`
- Spec requires row 3 to demonstrate `bold=true` with `fg=Indexed(4)` (blue). The current row 3 uses `Indexed(14)` (bright cyan) with no bold. Since bold rendering is a feature of `arcterm-core::CellAttrs`, the example should exercise it.

---

## Summary
**Verdict: BLOCK**

Three spec violations prevent approval: `begin_frame` must return `Result` to enable `SurfaceError::Lost` recovery as explicitly required by the spec; `TextRenderer::render` signature and atlas-trim placement deviate from spec; and `about_to_wait` is absent, meaning the example does not continuously redraw as required. Fix C1 and C2 (both are small targeted changes) and reconcile the `TextRenderer::render` signature per I1, then resubmit.

Critical: 2 | Important: 3 | Suggestions: 0
