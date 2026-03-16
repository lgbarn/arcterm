# Research: Phase 11 — Config and Runtime Hardening (M-3, M-4, M-5)

## Context

Arcterm is a GPU-accelerated terminal emulator organized as a six-crate Cargo workspace.
Phase 11 addresses three medium-severity concerns identified in CONCERNS.md that were deferred
until Phases 9 and 10 completed to avoid file conflicts. All three fixes are in the
`arcterm-app` / `arcterm-render` layer:

- **M-3**: Kitty image PNG/JPEG decode runs synchronously inline in the PTY processing loop
  (`terminal.rs:92`), blocking the main event loop on large images.
- **M-4**: `scrollback_lines` parsed from config TOML is applied directly to `Grid::max_scrollback`
  with no upper-bound cap, allowing unbounded memory growth.
- **M-5**: Three `.expect()` calls in `GpuState::new()` (`gpu.rs:29,38,50`) panic the process
  on any GPU initialization failure with no user-facing error message.

The design decisions for all three fixes are already recorded in `CONTEXT-11.md`.
This document captures the concrete code-level findings needed to implement those decisions.

---

## M-3: Async Kitty Image Decode

### Current synchronous path

**File:** `arcterm-app/src/terminal.rs`

The synchronous decode lives entirely in `Terminal::process_pty_output()` (lines 79–113).
After the APC scanner dispatches Kitty payloads and the `KittyChunkAssembler` reassembles
multi-chunk transfers, `image::load_from_memory(&decoded_bytes)` is called inline. On success,
a `PendingImage` is pushed into `self.pending_images: Vec<PendingImage>`.

The `Terminal` struct currently owns the accumulator vec directly:

```
Terminal {
    pty: PtySession,
    scanner: ApcScanner,
    grid_state: GridState,
    chunk_assembler: KittyChunkAssembler,
    pending_images: Vec<PendingImage>,    // <-- synchronous staging area
}
```

There is an existing TODO at line 37–39 documenting this limitation:
`// TODO(phase-5): move PNG/JPEG decoding to a background thread for images larger than 1MB`.

**`PendingImage` struct** (lines 14–23): plain data — `command: KittyCommand`, `rgba: Vec<u8>`,
`width: u32`, `height: u32`. No lifetime dependencies. Fully `Send`-safe (all fields are owned).
`KittyCommand` must be confirmed `Send`; it comes from `arcterm-vt` and is a plain parsed struct.

### Drain call site in main.rs

`terminal.take_pending_images()` is called in `about_to_wait()` at line 1452, inside the
per-pane PTY drain loop (after `terminal.process_pty_output(&bytes)`). The images are then
passed immediately to `state.renderer.upload_image()` and pushed to
`state.renderer.image_placements`.

The drain happens in the `AboutToWait` handler on the main thread — the same thread that
calls `Renderer::render_frame()` in `RedrawRequested`. There is no frame-boundary separation
between when `take_pending_images()` is called and when the renderer uses the results.

### Async infrastructure already present

- `tokio` with `features = ["full"]` is a workspace dependency and is already imported in
  `arcterm-app/src/main.rs` (`use tokio::sync::mpsc;`) and `terminal.rs`
  (`use tokio::sync::mpsc;`).
- `tokio::task::spawn_blocking` is already used in `arcterm-plugin/src/manager.rs:507` for
  WASM invocation — the pattern is established in the codebase.
- The tokio multi-thread runtime is created in `main()` before the winit event loop.
- No `JoinHandle`, `decode_image_task`, or `image_decode` symbols exist anywhere — there is
  zero async image infrastructure today.

### Required structural change

The CONTEXT-11 decision: use `tokio::task::spawn_blocking` with an mpsc channel.

**What changes in `terminal.rs`:**

1. Add a `tokio::sync::mpsc::Sender<PendingImage>` field to `Terminal` (replacing or alongside
   `pending_images: Vec<PendingImage>`).
2. Remove the `pending_images` vec and `take_pending_images()` method, or repurpose them.
3. In `process_pty_output()`, replace the inline `image::load_from_memory` call with a
   `tokio::task::spawn_blocking` call that moves `decoded_bytes` and `meta` into the closure,
   decodes there, and sends a `PendingImage` through the channel on success.
4. The `Terminal::new()` constructor must create the `(tx, rx)` pair and return (or store) the
   receiver for the app layer.

**What changes in `main.rs`:**

The drain call at line 1452 changes from:
```rust
let pending = terminal.take_pending_images();
```
to a non-blocking drain of the mpsc receiver:
```rust
loop {
    match image_rx.try_recv() {
        Ok(img) => { /* upload_image + push placement */ }
        Err(_) => break,
    }
}
```
The `image_rx` receiver needs to be stored somewhere accessible in `AboutToWait`. Options:
- Store it in `Terminal` itself (simplest — caller retrieves with a drain method).
- Store it alongside the `pty_channels` map (same pattern as the PTY byte receiver).

The existing `pty_channels: HashMap<PaneId, tokio::sync::mpsc::Receiver<Vec<u8>>>` map
(line 332 in main.rs type alias `PaneBundle`) is exactly this pattern. A parallel
`image_channels: HashMap<PaneId, tokio::sync::mpsc::Receiver<PendingImage>>` map is the
most consistent approach with existing conventions.

**One-frame latency note:** Because `spawn_blocking` is asynchronous, decode results will not
be available on the same `AboutToWait` tick that triggered the PTY batch. They will arrive
on the next tick (or later). This matches the ROADMAP risk description: "the async replacement
must handle the case where decoded images arrive one frame late." The try_recv drain before each
render frame handles this correctly — images appear on the frame after decode completes.

### Imports needed in terminal.rs

`tokio::sync::mpsc` is already imported. No new crate dependencies required.
`tokio::task::spawn_blocking` is accessed via `tokio::task::spawn_blocking` — available from
the existing `tokio = { version = "1", features = ["full"] }` workspace dep.

### Existing tests

No tests exist for `terminal.rs` — neither a `mod tests` block nor any integration tests
referencing `process_pty_output` or `take_pending_images` were found. The `#[allow(dead_code)]`
on `take_pending_images` confirms it has never been exercised by a test.

The regression test for M-3 must be a unit test in `terminal.rs` or an integration test.
Given that `Terminal::new()` requires a live PTY process (spawns a shell), the most practical
approach is a unit test that directly exercises `process_pty_output` with a raw Kitty APC
payload (bypassing the PTY) and asserts that the image receiver eventually yields a
`PendingImage`. This requires a small tokio test runtime (`#[tokio::test]`).

---

## M-4: Scrollback Lines Config Cap

### Where `scrollback_lines` is defined

**File:** `arcterm-app/src/config.rs:32`

```rust
pub scrollback_lines: usize,
```

Default: `10_000` (line 60). The field is a bare `usize` with no validation annotation.
`#[serde(default)]` on the struct allows omission but permits any non-negative integer.

### Where the value is applied

`max_scrollback` is set in **four places** in `arcterm-app/src/main.rs`:

| Line | Context |
|------|---------|
| 351 | Initial single-pane spawn in `resumed()` |
| 832 | `open_pane()` helper |
| 912 | Workspace restore loop |
| 1067 | Workspace restore loop (second path, CLI workspace open) |
| 1298 | Hot-reload handler (config change updates all live panes) |

All five assignment sites do `terminal.grid_mut().max_scrollback = cfg.scrollback_lines` (or
`new_cfg.scrollback_lines`).

### Where the cap is enforced in `arcterm-core`

**File:** `arcterm-core/src/grid.rs:208`

```rust
while self.scrollback.len() > self.max_scrollback {
    self.scrollback.pop_back();
}
```

The cap is enforced correctly at scroll time, but `max_scrollback` itself is uncapped — it is
set directly from whatever value the user provides. A value of `usize::MAX` would prevent the
scrollback from ever being trimmed (the `while` condition would never be true since `len()` can
never exceed `usize::MAX`), effectively making scrollback unbounded.

### Correct fix location

The cap must be applied **in `ArctermConfig::load()`** (and `load_with_overlays()`), not at
the assignment sites in `main.rs`. Applying it at the config layer means:

1. All five assignment sites are automatically correct.
2. The hot-reload path at line 1298 also receives the capped value.
3. The warning can be logged once, at load time, rather than at each assignment site.

The alternative — capping at each assignment site in `main.rs` — would require changes to
five locations, risk missing future assignment sites, and produce no warning unless separately
added.

The CONTEXT-11 decision (cap at 1,000,000) aligns with the CONCERNS.md remediation.
`1_000_000` is the correct constant: generous enough (~400 MB worst case at 80 cols) and
clearly out of reach of any accidental typo.

### Existing tests

`config.rs` has a thorough test suite (lines 418–705). The relevant existing tests:
- `defaults_are_sensible` asserts `scrollback_lines == 10_000`.
- `toml_overrides_fields` asserts `scrollback_lines == 50_000` (reasonable value, not near cap).

No test currently exercises an extreme value. The regression test for M-4 must add a case
that parses `scrollback_lines = 999999999999` from TOML and asserts:
1. The parsed value is clamped to `1_000_000`.
2. The test does not depend on real log output (log macros in tests require `env_logger::init()`
   or a test logger — acceptable to test only the clamping, not the warning text).

### Note on `load_with_overlays`

`load_with_overlays()` uses `merged.clone().try_into::<Self>().unwrap_or_default()` (line 299).
The cap must be applied after deserialization in both `load()` and `load_with_overlays()`.
A private helper `fn clamp_validated(mut self) -> Self` called at the end of both functions
is the cleanest pattern.

---

## M-5: GPU Init Safety

### Current panic surface in `gpu.rs`

**File:** `arcterm-render/src/gpu.rs`

Three `.expect()` calls in `new_async()` (the async body of `GpuState::new()`):

| Line | Call | Panic message |
|------|------|---------------|
| 29 | `instance.create_surface(window.clone()).expect(...)` | `"failed to create wgpu surface"` |
| 38 | `instance.request_adapter(...).await.expect(...)` | `"failed to find a suitable GPU adapter"` |
| 50 | `adapter.request_device(...).await.expect(...)` | `"failed to create wgpu device"` |

`GpuState::new()` currently returns `Self` (line 17). `new_async()` returns `Self` (line 21).

There is also a soft fallback on line 54:
```rust
let surface_format = caps.formats.first().copied()
    .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);
```
This is not a panic — it already handles the absent-formats case gracefully.

### Call chain

```
GpuState::new(window)                    // arcterm-render/src/gpu.rs:17
  └─ pollster::block_on(Self::new_async) // arcterm-render/src/gpu.rs:18
       ↑
Renderer::new(window, font_size)         // arcterm-render/src/renderer.rs:85
  └─ let gpu = GpuState::new(window);   // arcterm-render/src/renderer.rs:85
       ↑
App::resumed()                           // arcterm-app/src/main.rs:1003
  └─ let mut renderer = Renderer::new(window.clone(), cfg.font_size);
```

`Renderer::new()` returns `Self` (line 84 of renderer.rs). `App::resumed()` does not handle
any error from `Renderer::new()` — it simply assigns the return value.

### Propagation scope

Changing `GpuState::new()` to `-> Result<GpuState, String>` requires:

1. **`arcterm-render/src/gpu.rs`**: Change `new()` and `new_async()` signatures; replace
   three `.expect()` calls with `?` (map errors to `String` with `.map_err(|e| e.to_string())`
   or similar).
2. **`arcterm-render/src/renderer.rs`**: Change `Renderer::new()` return type to
   `Result<Self, String>`; propagate the `GpuState::new()` result with `?`.
3. **`arcterm-app/src/main.rs:1003`**: Handle the `Result` from `Renderer::new()` — display
   a user-facing error message and call `event_loop.exit()` on `Err`.

The ROADMAP confirms: "M-5 changes constructor signatures in `arcterm-render` which propagate
to `arcterm-app` — verify all call sites are updated."

### Error type choice

The codebase uses `String` errors for renderer-level failures (no `anyhow` in
`arcterm-render`'s `Cargo.toml`). `-> Result<GpuState, String>` is consistent with the
existing error handling in `renderer.rs` (e.g., `begin_frame` returns
`Result<..., wgpu::SurfaceError>`). Using `String` avoids adding `anyhow` as a dependency
to `arcterm-render`.

### `create_surface` error type

`instance.create_surface()` returns `Result<Surface, CreateSurfaceError>`.
`CreateSurfaceError` implements `Display` so `.map_err(|e| e.to_string())` works.

`request_adapter()` returns `Option<Adapter>` (not `Result`). The `.expect()` pattern
must be replaced with `.ok_or_else(|| "failed to find a suitable GPU adapter".to_string())`.

`request_device()` returns `Result<(Device, Queue), RequestDeviceError>`.
`RequestDeviceError` implements `Display`.

### User-facing error in `main.rs`

The CONCERNS.md remediation calls for "a user-friendly dialog or log fatal message before
exiting." In a `winit` app before the first frame, a dialog is not yet available. The correct
approach: log a `log::error!` with the GPU error string, then call `event_loop.exit()`. A
message box via a platform API would be ideal but is out of scope for this phase.

### Existing tests

No tests exist in `gpu.rs` (confirmed: no `#[cfg(test)]` block). `arcterm-render` has a
`gpu-tests` feature flag (in `Cargo.toml`) suggesting GPU tests were anticipated but not yet
written.

The ROADMAP success criteria state: "unit test that `GpuState::new` returns `Err` on invalid
adapter request if testable, otherwise integration-level assertion." Testing GPU init in CI
without a display is unreliable. The most feasible test: a compile-time check that
`Renderer::new()` returns `Result<_, _>` (enforced by callers using `?` or `match`), plus a
documentation test in the doc comment of `GpuState::new` showing the `Result` usage.

---

## Comparison Matrix

| Criteria | M-3: Async Image Decode | M-4: Scrollback Cap | M-5: GPU Init Safety |
|----------|------------------------|---------------------|----------------------|
| Files changed | `terminal.rs`, `main.rs` | `config.rs` | `gpu.rs`, `renderer.rs`, `main.rs` |
| Lines of change | ~30–40 | ~10–15 | ~20–25 |
| New crate deps | None | None | None |
| API signature change | Yes — `Terminal` gains channel; `take_pending_images` removed/replaced | No — `scrollback_lines` field unchanged, post-load clamping added | Yes — `GpuState::new` and `Renderer::new` return `Result` |
| Cross-crate impact | `arcterm-app` only | `arcterm-app` only | `arcterm-render` + `arcterm-app` |
| Test approach | `#[tokio::test]` unit test with synthetic Kitty APC payload | Config parse test with extreme `scrollback_lines` value | Compile-time + doc test; GPU hardware unavailable in CI |
| Relative complexity | High (async data-flow change) | Low (validation helper) | Medium (signature propagation) |
| Risk | One-frame decode latency; must not drop images | None — purely additive validation | Must update all call sites; winit `resumed()` error path |

---

## Implementation Considerations

### M-3: Execution order and channel ownership

The `Terminal` struct is owned inside `AppState.panes: HashMap<PaneId, Terminal>`. The
PTY byte receivers are stored separately in `AppState.pty_channels: HashMap<PaneId, Receiver<Vec<u8>>>`.
The image decode receivers must follow the same pattern — stored in a parallel
`image_rx_channels: HashMap<PaneId, Receiver<PendingImage>>` — or embedded in `Terminal`
as a field that exposes a drain method. The parallel map is more consistent with existing
conventions and avoids making `Terminal` responsible for non-PTY async state.

When a pane is closed (`close_pane()`), the corresponding entry in `image_rx_channels` must
also be removed to avoid holding the receiver open after the pane's `Terminal` is dropped.
The sender embedded in `Terminal` will be dropped when the `Terminal` is dropped, which will
close the channel naturally — the receiver entry in the map will then return `Disconnected`.

### M-3: `#[allow(dead_code)]` cleanup

Both `pending_images: Vec<PendingImage>` and `take_pending_images()` carry `#[allow(dead_code)]`
annotations (lines 13 and 116). These must be removed as part of the refactor. If the new
channel-based field is used immediately in `main.rs`, no `allow(dead_code)` is needed.

### M-4: Two load paths

Both `ArctermConfig::load()` (line 175) and `ArctermConfig::load_with_overlays()` (line 258)
produce an `ArctermConfig`. The cap must be applied in both. A private method:

```rust
fn validate(mut self) -> Self {
    const MAX_SCROLLBACK: usize = 1_000_000;
    if self.scrollback_lines > MAX_SCROLLBACK {
        log::warn!(
            "config: scrollback_lines {} exceeds maximum {}; clamping",
            self.scrollback_lines, MAX_SCROLLBACK
        );
        self.scrollback_lines = MAX_SCROLLBACK;
    }
    self
}
```

called at the end of both `load()` (replacing `Ok(cfg)` with `Ok(cfg.validate())`) and
`load_with_overlays()` (replacing the final `(cfg, merged)` with `(cfg.validate(), merged)`)
ensures no load path is missed.

The constant `MAX_SCROLLBACK: usize = 1_000_000` should be a module-level `const` or a
private associated constant on `ArctermConfig` for documentation purposes.

### M-5: `pollster::block_on` and async error propagation

`GpuState::new()` wraps `new_async()` via `pollster::block_on()`. The propagation is:

```rust
pub fn new(window: Arc<Window>) -> Result<Self, String> {
    pollster::block_on(Self::new_async(window))
}

async fn new_async(window: Arc<Window>) -> Result<Self, String> {
    // replace .expect() calls with ? via .map_err(|e| e.to_string())
}
```

`Renderer::new()` becomes:

```rust
pub fn new(window: Arc<Window>, font_size: f32) -> Result<Self, String> {
    let gpu = GpuState::new(window)?;
    // ... rest unchanged
    Ok(Self { gpu, text, quads, images, ... })
}
```

In `App::resumed()` in `main.rs`:

```rust
let mut renderer = match Renderer::new(window.clone(), cfg.font_size) {
    Ok(r) => r,
    Err(e) => {
        log::error!("GPU initialization failed: {e}");
        event_loop.exit();
        return;
    }
};
```

### M-5: Test feasibility

A GPU hardware test cannot run headlessly in CI. The ROADMAP acknowledges this: "unit test
that `GpuState::new` returns `Err` on invalid adapter request if testable, otherwise
integration-level assertion." The achievable test is a doc-comment example showing
`GpuState::new` returns `Result`, and a compile-time assertion that `Renderer::new` now
returns `Result`. No runtime GPU test is expected for Phase 11.

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| M-3: Decode channel sender closed before images drain (Terminal dropped before drain runs) | Low | Medium | Drain `image_rx` in `about_to_wait` before removing pane entries; channel close is detectable via `Disconnected` |
| M-3: `spawn_blocking` thread pool exhaustion under burst of large images | Low | Low | `tokio` blocking thread pool is elastic; each decode is short-lived. Log a warning if channel backpressure exceeds threshold |
| M-3: `KittyCommand` not `Send` | Very Low | High | All fields of `KittyCommand` are plain data (`u32`, `String`, enums) — `Send` is auto-derived. Confirmed by compiler if `spawn_blocking` closure captures it |
| M-4: Hot-reload path bypasses `validate()` | Low | Medium | Cap applied in `load()` which is called by `watch_config()`'s reload loop (line 400 of config.rs) — confirmed |
| M-4: `usize::MAX` parsed from TOML | Very Low | Medium | TOML integers are i64 on most platforms; values exceeding `i64::MAX` will fail TOML deserialization, not reach the validator. The cap still handles the realistic range |
| M-5: Missing call site in `main.rs` causes compile error | Low | Low | Rust type system will catch any call site that discards the new `Result` with a "must use" warning (-D warnings enforced by clippy) |
| M-5: winit `resumed()` called multiple times (some platforms) | Low | Low | The `if self.state.is_some() { return; }` guard at line 980 prevents double-init |

---

## Sources

All findings are from direct code inspection. No external URLs were consulted; all evidence
is from the local codebase.

1. `arcterm-app/src/terminal.rs` — synchronous decode path, `PendingImage` struct, `take_pending_images`
2. `arcterm-app/src/main.rs:1003` — `Renderer::new()` call site in `resumed()`
3. `arcterm-app/src/main.rs:1452` — `take_pending_images()` drain in `about_to_wait`
4. `arcterm-app/src/config.rs:32,60,175,258` — `scrollback_lines` field, default, `load()`, `load_with_overlays()`
5. `arcterm-app/src/main.rs:351,832,912,1067,1298` — `max_scrollback` assignment sites
6. `arcterm-core/src/grid.rs:80,112,208` — `max_scrollback` field, default, enforcement loop
7. `arcterm-render/src/gpu.rs:17–83` — `GpuState::new()`, `new_async()`, three `.expect()` calls
8. `arcterm-render/src/renderer.rs:84–103` — `Renderer::new()` calling `GpuState::new()`
9. `arcterm-plugin/src/manager.rs:507` — existing `tokio::task::spawn_blocking` pattern
10. `arcterm-app/Cargo.toml` — confirms `tokio.workspace = true`, `image.workspace = true`
11. `Cargo.toml` (workspace) — `tokio = { version = "1", features = ["full"] }`
12. `arcterm-render/Cargo.toml` — confirms `pollster.workspace = true`, no `anyhow` dep
13. `.shipyard/ROADMAP.md:332–357` — Phase 11 scope and success criteria
14. `.shipyard/codebase/CONCERNS.md:64–87` — M-3, M-4, M-5 original findings
15. `.shipyard/phases/11/CONTEXT-11.md` — design decisions

---

## Uncertainty Flags

- **M-3 test feasibility**: A unit test for `process_pty_output` with async decode requires
  either a live PTY (spawns a shell — fragile in CI) or a refactor that makes `process_pty_output`
  testable without a PTY. The cleanest approach is to extract the Kitty decode dispatch into
  a free function that accepts `decoded_bytes` and `meta` and returns a `JoinHandle`, then test
  that function directly. Whether this extraction is within Phase 11 scope should be confirmed.

- **M-3 channel backpressure**: The mpsc channel capacity is unspecified in the design. An
  unbounded channel avoids blocking `process_pty_output` (which runs on the main thread) but
  permits unbounded memory use during image bursts. A bounded channel (e.g., `mpsc::channel(32)`)
  would require `try_send` in `spawn_blocking` which adds complexity. The correct bound is
  unclear; this should be decided before implementation.

- **M-5 CI test strategy**: The ROADMAP success criterion says "unit test that `GpuState::new`
  returns `Err` on invalid adapter request if testable." No CI GPU adapter is available in
  typical macOS or Linux CI environments. It is unclear whether the `gpu-tests` feature flag in
  `arcterm-render/Cargo.toml` was intended for optional GPU tests gated behind that flag.
  The test plan should explicitly state whether the M-5 test is deferred or replaced by a
  compile-time check.

- **M-4 `load_with_overlays` warning duplication**: If an overlay file sets `scrollback_lines`
  to an extreme value, the warning will fire when `load_with_overlays()` is called. If
  `load()` is called separately on the same config file, the warning fires again. Whether
  duplicate warnings are acceptable or should be deduplicated is not specified.
