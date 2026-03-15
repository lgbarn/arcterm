# SUMMARY-3.2 — Kitty Graphics Protocol (Inline Images)

**Plan:** PLAN-3.2
**Phase:** 4
**Wave:** 3
**Date:** 2026-03-15
**Status:** Complete

---

## What Was Done

### Task 1 — Kitty APC payload parser and chunk assembler (TDD)

**File created:** `arcterm-vt/src/kitty.rs`
**File modified:** `arcterm-vt/src/lib.rs`

Implemented the full Kitty Graphics Protocol parsing layer:

- `KittyAction` enum: `TransmitAndDisplay`, `Transmit`, `Display`, `Delete`, `Unknown`
- `KittyFormat` enum: `Png`, `Rgb24`, `Rgba32`, `Unknown`
- `KittyCommand` struct with all control-data fields: `action`, `format`, `image_id`, `more_chunks`, `quiet`, `cols`, `rows`, `payload_base64`
- `parse_kitty_command(raw: &[u8]) -> Option<KittyCommand>`:
  - Strips optional leading `G` prefix
  - Splits on first `;` to separate control-data from base64 payload
  - Parses comma-separated `key=value` pairs from control-data
  - Maps `a`, `f`, `i`, `m`, `q`, `c`, `r` keys to typed fields
- `KittyChunkAssembler` struct with `pending: HashMap<u32, Vec<u8>>`:
  - `receive_chunk(&cmd) -> Option<(KittyCommand, Vec<u8>)>`:
    - If `more_chunks=true`: appends payload bytes to pending buffer, returns `None`
    - If `more_chunks=false`: concatenates all pending + current, decodes base64, clears buffer, returns decoded bytes

All tests were written first (TDD), then implemented: **17 tests pass**.

Test coverage:
- `parse_transmit_and_display_png_quiet1` — action, format, quiet parsing
- `parse_with_g_prefix` — optional G prefix stripping
- `parse_chunked_first_chunk` — image_id and more_chunks
- `parse_no_semicolon_no_payload` — control-only payload
- `parse_unknown_action` / `parse_unknown_format` — graceful unknown handling
- `parse_delete_action`, `parse_display_action`, `parse_cols_rows`, `parse_rgb24_format`, `parse_rgba32_format`
- `single_chunk_m0_returns_immediately` — immediate decode
- `single_chunk_m1_returns_none` — buffering
- `two_chunks_m1_then_m0_assembles_correctly` — 2-chunk assembly
- `three_chunks_assembles_correctly` — 3-chunk assembly with correct concatenated decode
- `different_image_ids_do_not_interfere` — interleaved transfers
- `pending_cleared_after_final_chunk` — no memory leak

**Commit:** `shipyard(phase-4): add Kitty graphics APC payload parser and chunk assembler`

---

### Task 2 — ImageQuadRenderer with textured wgpu pipeline

**File created:** `arcterm-render/src/image_quad.rs`
**File modified:** `arcterm-render/src/lib.rs`

Implemented a separate wgpu render pipeline (distinct from `QuadRenderer`) for rendering RGBA images as textured quads:

- `ImageVertex { position: [f32; 2], uv: [f32; 2] }` — bytemuck Pod/Zeroable vertex type
- `ScreenUniform { resolution: [f32; 2], _pad: [f32; 2] }` — 16-byte aligned viewport uniform
- `ImageTexture { texture, view, bind_group, width, height }` — one uploaded GPU image
- `ImageQuadRenderer`:
  - Linear + clamp-to-edge sampler
  - Bind group layout: group 0 = viewport uniform; group 1 = texture + sampler
  - WGSL shader: pixel-space → clip-space vertex transform + `textureSample` fragment
  - `create_texture(device, queue, rgba_bytes, width, height) -> ImageTexture`:
    - Computes `aligned_bpr = (width * 4 + 255) & !255` for wgpu's 256-byte requirement
    - Pads rows in a temporary buffer when alignment requires it
    - Uses `Texture::as_image_copy()` and `TexelCopyBufferLayout` (wgpu 28 API)
  - `prepare(queue, placements, viewport_w, viewport_h) -> usize`: uploads all vertices + uniform before the render pass
  - `render(pass, placements)`: issues one draw call per image using per-image bind groups

**wgpu 28 API deviations from initial code:** `ImageCopyTexture` → `Texture::as_image_copy()`, `ImageDataLayout` → `TexelCopyBufferLayout`, `FilterMode::Nearest` (for mipmap) → `MipmapFilterMode::Nearest`, removed `push_constant_ranges` field (field was renamed in wgpu 28).

**Commit:** `shipyard(phase-4): add ImageQuadRenderer textured wgpu pipeline for inline images`

---

### Task 3 — Integration

**Files modified:**
- `arcterm-vt/src/handler.rs` — GridState kitty payload storage
- `arcterm-app/src/terminal.rs` — Terminal with chunk assembler + image decoding
- `arcterm-app/src/main.rs` — App loop: drain images → upload → place
- `arcterm-app/Cargo.toml` — added `image.workspace = true`
- `arcterm-render/src/renderer.rs` — Renderer with ImageQuadRenderer + image_store + upload_image

**Changes:**

**arcterm-vt/handler.rs** — Added `kitty_payloads: Vec<Vec<u8>>` field to `GridState` and `take_kitty_payloads()` drain method. Implemented `kitty_graphics_command` on `GridState`'s `Handler` impl to push raw APC payloads into `kitty_payloads` for the app layer.

**arcterm-app/terminal.rs** — Added:
- `PendingImage { command, rgba, width, height }` — decoded image ready for GPU
- `chunk_assembler: KittyChunkAssembler` field on `Terminal`
- `pending_images: Vec<PendingImage>` field on `Terminal`
- Extended `process_pty_output` to drain `kitty_payloads`, parse with `parse_kitty_command`, feed through `chunk_assembler`, decode PNG/JPEG via `image::load_from_memory(...).to_rgba8()`, and append to `pending_images`
- `take_pending_images() -> Vec<PendingImage>` drain method

**arcterm-render/renderer.rs** — Extended `Renderer` with:
- `images: ImageQuadRenderer` — textured pipeline instance
- `image_store: HashMap<u32, ImageTexture>` — per image_id GPU textures
- `image_placements: Vec<(u32, [f32; 4])>` — current-frame placements
- `upload_image(id, rgba, w, h)` — uploads RGBA to GPU, replaces existing entry
- In `render_multipane`: collects `(&ImageTexture, [f32; 4])` references from `image_store`, calls `images.prepare()` before the pass, calls `images.render()` inside the pass between cell quads and text

**arcterm-app/main.rs** — After `process_pty_output`, drains `take_pending_images`, calls `renderer.upload_image` for each, pushes placements to `renderer.image_placements`. At render time, clears `image_placements` before building the frame.

**Commit:** `shipyard(phase-4): wire Kitty graphics end-to-end: parse→decode→GPU texture→render`

---

## Verification Results

| Task | Verify Command | Result |
|------|---------------|--------|
| 1 | `cargo test -p arcterm-vt -- kitty` | 17 passed, 0 failed |
| 2 | `cargo build -p arcterm-render` | Finished, 0 errors |
| 3 | `cargo build -p arcterm-app && cargo clippy --workspace -- -D warnings` | Both clean |

---

## Deviations from Plan

1. **wgpu 28 API changes** (inline fix): The plan referenced `ImageCopyTexture`, `ImageDataLayout`, and `push_constant_ranges` which were renamed/removed in wgpu 28. Fixed inline to use `Texture::as_image_copy()`, `TexelCopyBufferLayout`, and `..Default::default()` respectively.

2. **prepare/render split** (design improvement): The plan described a single `render()` method that took `queue` for `write_buffer` calls during the pass. wgpu 28 discourages queue writes during an active render pass; restructured into `prepare()` (uploads before pass) + `render()` (draws inside pass) matching the `QuadRenderer` pattern.

3. **image_placements clearing** (operational correctness): The plan did not specify when to clear `image_placements` between frames. Added a `retain(|_| false)` clear at the start of each redraw cycle. Full persistent placement tracking keyed by image_id is a Phase 5 enhancement (noted with TODO).

4. **Terminal file already uses ApcScanner + GridState**: The plan's Task 3 described migrating from `Processor`/`Grid` to `ApcScanner`/`GridState`, but this migration had already been completed in PLAN-3.1. The kitty integration was added on top of the existing architecture without needing to repeat that work.

5. **image crate in arcterm-app**: The plan noted the image crate is in `arcterm-render` deps. Added it to `arcterm-app/Cargo.toml` as a workspace dependency since image decoding happens in the app layer (Terminal struct).

---

## Files Touched

| File | Change |
|------|--------|
| `arcterm-vt/src/kitty.rs` | Created — parser, enums, chunk assembler, 17 tests |
| `arcterm-vt/src/lib.rs` | Added `pub mod kitty` + 5 public exports |
| `arcterm-vt/src/handler.rs` | Added `kitty_payloads` field, `take_kitty_payloads()`, `kitty_graphics_command` impl |
| `arcterm-render/src/image_quad.rs` | Created — ImageQuadRenderer, ImageTexture, WGSL shader |
| `arcterm-render/src/lib.rs` | Added `pub mod image_quad` + 3 public exports |
| `arcterm-render/src/renderer.rs` | Added `images`, `image_store`, `image_placements`, `upload_image`, render wiring |
| `arcterm-app/src/terminal.rs` | Added `PendingImage`, `chunk_assembler`, `pending_images`, Kitty decode path |
| `arcterm-app/src/main.rs` | Wired pending image drain + upload + placement registration |
| `arcterm-app/Cargo.toml` | Added `image.workspace = true` |
