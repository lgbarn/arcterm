---
phase: structured-output
plan: "3.2"
wave: 3
dependencies: ["1.2", "2.1"]
must_haves:
  - Kitty graphics APC payload parsing (control-data key=value + base64 payload)
  - PNG/JPEG decoding from base64 payload to RGBA pixel buffer
  - wgpu texture creation from decoded image bytes
  - ImageQuadRenderer with textured pipeline (vertex + UV + sampler + bind group)
  - Image displayed inline at grid position
  - Chunked transfer support (m=0/m=1)
files_touched:
  - arcterm-vt/src/kitty.rs (new)
  - arcterm-vt/src/lib.rs
  - arcterm-render/src/image_quad.rs (new)
  - arcterm-render/src/renderer.rs
  - arcterm-render/src/lib.rs
  - arcterm-app/src/terminal.rs
  - arcterm-app/src/main.rs
tdd: false
---

# PLAN-3.2 -- Kitty Graphics Protocol (Inline Images)

## Goal

Implement basic Kitty graphics protocol support: receive PNG/JPEG image data via APC escape sequences, decode to RGBA pixels, upload as wgpu textures, and render as inline quads at grid positions. This enables AI tools and CLI programs to display images inline in the terminal.

## Why Wave 3

Kitty graphics depends on the APC scanner from PLAN-1.2 and shares the overlay rendering architecture established in PLAN-3.1. The textured quad pipeline is independent from the text/quad pipelines and can be built in parallel with PLAN-3.1.

## Design Notes

**Payload parsing**: The APC scanner delivers raw bytes after `ESC _ G`. The format is `<key=val,key=val,...>;<base64-data>`. Parse control data, decode base64, then act based on `a=` (action), `f=` (format), `m=` (more chunks), `i=` (image ID).

**Chunked transfers**: When `m=1`, buffer the base64 chunk. When `m=0` (or absent), concatenate all chunks for this image ID, decode base64, and process the image.

**Texture lifecycle**: Each decoded image becomes a `wgpu::Texture` + `TextureView` + `BindGroup`. These are stored in an `ImageStore` per pane, keyed by image ID. When a pane is closed or the image scrolls beyond scrollback, the texture is dropped (releasing GPU memory).

**ImageQuadRenderer**: A separate wgpu render pipeline with a textured shader (vertex position + UV, fragment samples from a texture via a sampler). This is distinct from `QuadRenderer` (solid-color only). Each image is rendered as a single textured quad at its grid position.

**bytes_per_row alignment**: wgpu requires `bytes_per_row` to be a multiple of 256. The decoded RGBA buffer must be row-padded to meet this requirement.

## Tasks

<task id="1" files="arcterm-vt/src/kitty.rs, arcterm-vt/src/lib.rs, arcterm-vt/src/handler.rs" tdd="true">
  <action>Implement Kitty graphics payload parser and chunk assembler:

1. Create `arcterm-vt/src/kitty.rs` with:
   - `KittyCommand` struct: `action: KittyAction, format: KittyFormat, image_id: u32, more_chunks: bool, quiet: u8, cols: Option<u32>, rows: Option<u32>, payload_base64: Vec<u8>`.
   - `KittyAction` enum: `TransmitAndDisplay`, `Transmit`, `Display`, `Delete`, `Unknown`.
   - `KittyFormat` enum: `Png`, `Rgb24`, `Rgba32`, `Unknown`.
   - `parse_kitty_command(raw: &[u8]) -> Option<KittyCommand>`:
     - Split `raw` on first `;` to get control-data and base64-payload.
     - Parse control-data as comma-separated `key=value` pairs.
     - Map `a` key: `T` -> TransmitAndDisplay, `t` -> Transmit, `p` -> Display, `d` -> Delete.
     - Map `f` key: `100` -> Png, `24` -> Rgb24, `32` -> Rgba32.
     - Map `i` key to u32 image_id (default 0).
     - Map `m` key: `1` -> more_chunks=true, `0`/absent -> false.
     - Map `q` key to quiet level.
     - Map `c`, `r` keys to cols, rows.

2. `KittyChunkAssembler` struct:
   - `pending: HashMap<u32, Vec<u8>>` -- maps image_id to accumulated base64 bytes.
   - `fn receive_chunk(&mut self, cmd: &KittyCommand) -> Option<(KittyCommand, Vec<u8>)>`:
     - If `cmd.more_chunks`: append payload to pending[image_id], return None.
     - If not more_chunks: append payload, concatenate all pending chunks, decode base64, clear pending entry, return `Some((cmd_metadata, decoded_bytes))`.

3. Update `Handler` trait: change `kitty_graphics_command` signature to `fn kitty_graphics_command(&mut self, _command: KittyCommand, _decoded_image_bytes: Option<Vec<u8>>) {}` -- or keep it as raw bytes and parse in the app layer. Decision: parse in `kitty.rs`, deliver structured command.

4. Add `pub mod kitty;` to `arcterm-vt/src/lib.rs`. Export `KittyCommand`, `KittyChunkAssembler`, `parse_kitty_command`.

Write tests first:
- `parse_kitty_command(b"a=T,f=100,q=1;iVBORz...")` returns TransmitAndDisplay, Png format, quiet=1
- `parse_kitty_command(b"a=T,f=100,i=42,m=1;chunk1")` returns image_id=42, more_chunks=true
- Chunk assembler: 3 chunks with m=1,m=1,m=0 produces one complete decoded payload
- Chunk assembler: single chunk with m=0 returns immediately
- Parse with no semicolon (no payload) returns command with empty payload
- Parse with unknown action returns KittyAction::Unknown</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo test -p arcterm-vt -- kitty --nocapture</verify>
  <done>All Kitty command parsing and chunk assembly tests pass. Multi-chunk assembly produces correct concatenated payloads. Control-data key=value pairs parsed correctly.</done>
</task>

<task id="2" files="arcterm-render/src/image_quad.rs, arcterm-render/src/lib.rs" tdd="false">
  <action>Implement ImageQuadRenderer with a textured wgpu pipeline:

1. Create `arcterm-render/src/image_quad.rs` with:

2. `ImageTexture` struct: `texture: wgpu::Texture, view: wgpu::TextureView, bind_group: wgpu::BindGroup, width: u32, height: u32`.

3. `ImageQuadRenderer` struct:
   - `pipeline: wgpu::RenderPipeline`
   - `sampler: wgpu::Sampler`
   - `bind_group_layout: wgpu::BindGroupLayout`
   - `vertex_buffer: wgpu::Buffer`
   - `uniform_buffer: wgpu::Buffer` (for viewport dimensions)
   - `uniform_bind_group: wgpu::BindGroup`

4. WGSL shaders (inline strings):
   - Vertex shader: takes `position: vec2<f32>` and `uv: vec2<f32>`, outputs clip-space position and UV.
   - Fragment shader: samples from `texture_2d` + `sampler` using UV, outputs RGBA.

5. `ImageQuadRenderer::new(device, surface_format) -> Self`:
   - Create sampler (linear filtering, clamp-to-edge).
   - Create bind group layout with texture + sampler entries.
   - Create render pipeline with vertex layout `[position: f32x2, uv: f32x2]`.
   - Create vertex buffer (single quad, 6 vertices for two triangles).
   - Create uniform buffer (viewport width/height for pixel-to-clip-space conversion).

6. `ImageQuadRenderer::create_texture(&self, device, queue, rgba_bytes: &[u8], width: u32, height: u32) -> ImageTexture`:
   - Create texture with `TextureFormat::Rgba8UnormSrgb`.
   - Compute aligned `bytes_per_row = ((4 * width) + 255) & !255`.
   - If alignment requires padding, create a padded buffer with extra bytes per row.
   - Call `queue.write_texture(...)`.
   - Create texture view and bind group.

7. `ImageQuadRenderer::render(&self, pass: &mut RenderPass, textures: &[(ImageTexture, [f32; 4])])`:
   - For each (texture, rect), update vertex buffer with the rect coordinates converted to clip space, then draw.

8. Add `pub mod image_quad;` to `arcterm-render/src/lib.rs`.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-render 2>&1 | tail -5</verify>
  <done>`cargo build` succeeds. ImageQuadRenderer creates textured pipeline, uploads RGBA textures with correct alignment, and renders textured quads.</done>
</task>

<task id="3" files="arcterm-app/src/terminal.rs, arcterm-app/src/main.rs, arcterm-render/src/renderer.rs" tdd="false">
  <action>Wire Kitty graphics into the app and render pipeline:

1. In `arcterm-app/src/terminal.rs`:
   - Add `chunk_assembler: KittyChunkAssembler` field.
   - Add `pending_images: Vec<(KittyCommand, Vec<u8>)>` field for decoded images ready for texture upload.
   - In the `kitty_graphics_command` handler on GridState (or override in Terminal's processing): when ApcScanner delivers a kitty command, parse it with `parse_kitty_command`, feed to `chunk_assembler.receive_chunk`. If a complete image is returned, decode PNG/JPEG using `image::load_from_memory(&bytes)?.to_rgba8()` and store in `pending_images`.

2. In `arcterm-render/src/renderer.rs`:
   - Add `ImageQuadRenderer` to `Renderer` struct, initialized in `Renderer::new()`.
   - Add `ImageStore` (HashMap of image_id -> ImageTexture) to Renderer or pass via PaneRenderInfo.
   - In `render_multipane`, after drawing text, iterate pending images, create textures, and render image quads using `ImageQuadRenderer`.

3. In `arcterm-app/src/main.rs`:
   - In the PTY output processing path, after `process_pty_output`, drain `terminal.pending_images` and pass decoded RGBA bytes + placement info to the renderer.
   - In the render path, include image placements in the render call.

4. In `arcterm-vt/src/handler.rs`:
   - Implement `kitty_graphics_command` on GridState: store the raw command for the app layer to process.

5. Manual test: use a Kitty-graphics-aware tool (e.g., `kitty +kitten icat image.png`) or a test script that emits the correct APC sequence for a small PNG. Verify the image appears inline in the terminal.

Note on ISSUE items: this task adds `image` crate as a runtime dependency. Image decode should happen on a background thread for large images, but for Phase 4 basic support, synchronous decode on the PTY processing thread is acceptable for images under 1MB. Add a TODO comment for async decode in Phase 5+.</action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build -p arcterm-app 2>&1 | tail -5 && cargo clippy --workspace -- -D warnings 2>&1 | tail -5</verify>
  <done>Full workspace builds and clips clean. Kitty APC sequences are intercepted, parsed, chunk-assembled, decoded to RGBA, uploaded as wgpu textures, and rendered as inline image quads. Standard terminal output is unaffected.</done>
</task>
