# Research: Phase 4 — Structured Output and Smart Rendering

## Context

Arcterm is a Rust-based, wgpu-rendered terminal emulator. Phases 1–3 established: a VT parser (vte 0.15 crate), a terminal grid (`arcterm-core::Grid`), glyphon-based text rendering (`arcterm-render::TextRenderer`), a quad pipeline (`arcterm-render::QuadRenderer`), and a pane multiplexer. The renderer's frame entry point is `render_multipane`, which accepts `&[PaneRenderInfo]` and `&[OverlayQuad]`.

Phase 4 adds:
1. OSC 7770 protocol parser (custom typed content escape sequences)
2. Rich content renderers: code blocks (syntect), diffs, JSON, markdown, error, progress
3. Auto-detection engine (regex heuristics on unstructured output)
4. Kitty graphics protocol (basic PNG/JPEG inline images via APC escape sequences)
5. Content interaction: code block copy button (read-only; no collapse/expand)

Design decisions are already fixed (see CONTEXT-4.md): syntect for highlighting, regex heuristics, basic raster Kitty graphics, read-only interaction.

---

## Section 1: OSC 7770 Parser

### Codebase Integration Points

`arcterm-vt/src/processor.rs` — `Performer::osc_dispatch` is the sole OSC entry point. It receives `params: &[&[u8]]` where `params[0]` is the numeric identifier (e.g., `b"0"`, `b"2"`). Adding OSC 7770 is a match arm in the existing `match params[0]` block.

```
match params[0] {
    b"0" | b"2" => { ... }          // existing
    b"7770"     => { ... }          // add here
    _ => {}
}
```

`arcterm-vt/src/handler.rs` — The `Handler` trait defines all semantic terminal callbacks. A new `structured_content_start` and `structured_content_end` method pair (both with default no-op implementations) must be added here to follow the established pattern.

### vte OSC Buffer Constraints

The project uses `vte = "0.15"` with default features. The `std` feature is default in vte, which makes `osc_raw` an unbounded `Vec<u8>` (not the 1024-byte `ArrayVec` used in no-std mode). This means OSC 7770 payloads of arbitrary size are accumulated and delivered to `osc_dispatch` without truncation. The limit is MAX_OSC_PARAMS = 16 semicolon-delimited segments. The OSC 7770 format uses at most ~5 params (`7770 ; start ; type=code_block ; lang=rust ; ...`) — well within this limit.

### Protocol Format

```
ESC ] 7770 ; start ; type=<content_type> [; key=value]* ST
  <content as plain bytes — NOT inside another escape sequence>
ESC ] 7770 ; end ST
```

The content bytes between the two OSC sequences are plain PTY output, not an escape sequence. This means the VT parser processes them as `put_char` calls during the inter-sequence interval. The handler must buffer this content using a flag set by `structured_content_start` and flushed by `structured_content_end`.

This is a stateful accumulation problem. The `GridState` in `arcterm-vt/src/handler.rs` already holds state (scroll region, saved cursor, alt screen). A `StructuredContentAccumulator` field can be added to `GridState` using the same pattern.

### OSC 7770 vs APC

The OSC path is the correct choice for 7770. The vte crate's `SosPmApcString` state silently drops all APC data (ESC `_` ... ESC `\`) — there is no dispatch callback. This is confirmed by reading `alacritty/vte/src/lib.rs` directly: bytes in the `SosPmApcString` state route through `anywhere()`, which discards all non-control bytes. The OSC path is fully supported and accumulates to an unbounded Vec.

---

## Section 2: Syntax Highlighting — syntect

### Comparison Matrix

| Criteria | syntect 5.3.0 | tree-sitter 0.26.7 |
|---|---|---|
| Latest version | 5.3.0 (Sep 2025) | 0.26.7 (Mar 2026) |
| Total downloads | 12.2M | 15.1M |
| Recent downloads | 2.9M | 4.4M |
| License | MIT | MIT |
| Highlighting API | `HighlightLines::highlight_line` → `Vec<(Style, &str)>` | Requires per-language grammar crate + highlight query files |
| Bundled grammars | ~400+ languages via Sublime Text .tmLanguage | None bundled; each language is a separate crate |
| Bundled themes | Yes (Monokai, InspiredGitHub, Solarized) | No |
| Pure-Rust option | Yes (`default-fancy` feature, uses fancy-regex) | Yes (the core is pure Rust) |
| C binding risk | Default config uses onig (C); avoidable | None in core |
| Binary size impact | ~200 KB per bundled dump (dump-load) | Heavy: each grammar crate ~1–4 MB WASM/native |
| Primary users | bat, delta, mdBook | Neovim, Helix, GitHub |
| Learning curve | Low | High (requires query files, highlight config) |
| Stack compatibility | Direct RGBA color output via `Style.foreground` | Requires mapping node types to colors manually |

### syntect API Details

Key types:
- `syntect::parsing::SyntaxSet` — loaded via `SyntaxSet::load_defaults_newlines()`
- `syntect::highlighting::ThemeSet` — loaded via `ThemeSet::load_defaults()`
- `syntect::easy::HighlightLines` — stateful per-block highlighter
- `syntect::highlighting::Style` — fields: `foreground: Color`, `background: Color`, `font_style: FontStyle`
- `syntect::highlighting::Color` — fields: `r: u8`, `g: u8`, `b: u8`, `a: u8`
- `syntect::highlighting::FontStyle` — bitflags: `BOLD`, `UNDERLINE`, `ITALIC`

Workflow for a code block:
```rust
let ss = SyntaxSet::load_defaults_newlines();
let ts = ThemeSet::load_defaults();
let syntax = ss.find_syntax_by_extension("rs").unwrap_or_else(|| ss.find_syntax_plain_text());
let mut h = HighlightLines::new(syntax, &ts.themes["Monokai"]);
for line in code_block.lines() {
    let ranges: Vec<(Style, &str)> = h.highlight_line(line, &ss)?;
    // Each (Style, &str) maps directly to a glyphon Attrs span.
    // style.foreground.r/g/b → glyphon::Color::rgb(r, g, b)
    // style.font_style.contains(FontStyle::BOLD) → Attrs::new().weight(cosmic_text::Weight::BOLD)
}
```

The output maps directly to `buf.set_rich_text(...)` spans in the existing `shape_row_into_buffer` pattern.

Feature configuration for pure-Rust without C bindings:
```toml
syntect = { version = "5.3.0", default-features = false, features = ["default-fancy"] }
```

The `default-fancy` feature enables: `default-syntaxes`, `default-themes`, `dump-load`, `html`, `plist-load`, `yaml-load`, `regex-fancy` (pure Rust via fancy-regex).

### Recommendation: syntect

syntect is the correct choice. tree-sitter requires per-language grammar crates (each 1–4 MB), no bundled themes, and requires hand-authoring highlight query files. For a terminal emulator's Phase 4, syntect's direct RGBA span output maps trivially to existing glyphon `set_rich_text` calls. The `default-fancy` feature eliminates C dependencies.

---

## Section 3: Markdown Rendering — pulldown-cmark

### Comparison Matrix

| Criteria | pulldown-cmark 0.13.1 | comrak 0.31+ | markdown-it (N/A in Rust) |
|---|---|---|---|
| Latest version | 0.13.1 (Feb 2026) | 0.31+ | N/A |
| Total downloads | 74.8M | ~8M | N/A |
| Recent downloads | 17.2M | ~2M | N/A |
| License | MIT | MIT OR Apache-2.0 | N/A |
| CommonMark compliance | Yes | Yes (full) | N/A |
| API style | Pull parser (Iterator of Events) | AST (arena) | N/A |
| Allocation model | Zero-copy cow strings | Arena-based | N/A |
| Extensions | Tables, strikethrough, footnotes, task lists | Full GFM + extensions | N/A |
| Output targets | HTML (built-in), custom via events | HTML, AST | N/A |
| Stack fit | Events map directly to styled spans | AST adds extra step | N/A |
| Minimum Rust | 1.71.1 | 1.56+ | N/A |

### pulldown-cmark API Details

The parser is a Rust `Iterator` yielding `Event` variants:
- `Event::Start(Tag::Heading { level, .. })` / `Event::End(...)`
- `Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang)))` — fenced code blocks
- `Event::Text(CowStr)` — text content (may be multiple adjacent events; use `TextMergeStream`)
- `Event::Start(Tag::List(None))` / `Event::Start(Tag::Item)` — unordered lists
- `Event::Start(Tag::Strong)` / `Event::Start(Tag::Emphasis)` — bold/italic
- `Event::Code(CowStr)` — inline code spans

For Phase 4 (read-only rendering), the approach is:
1. Parse `Parser::new(text)` with `Options::all()` for tables and task lists
2. Walk events, building a `Vec<StyledSpan>` with (text, color, font_style, block_kind)
3. `StyledSpan` maps directly to a glyphon `Attrs` span in `set_rich_text`

The `TextMergeStream` utility in pulldown-cmark consolidates consecutive `Text` events, which matters for content correctness.

### Recommendation: pulldown-cmark

pulldown-cmark is the de facto standard (74.8M downloads, used by mdBook, Rustdoc, Zola, etc.). Its pull-parser event stream maps naturally to the span-building model the existing `shape_row_into_buffer` already uses. comrak is overkill for display-only rendering and adds an AST allocation layer without benefit here.

---

## Section 4: Kitty Graphics Protocol

### Protocol Summary

The Kitty graphics protocol transmits image data via APC escape sequences:
```
ESC _ G <control-data> ; <base64-payload> ESC \
```

Control data is comma-separated key=value pairs. Key parameters:
- `a=T` — transmit and display immediately
- `f=100` — PNG format; `f=24` = RGB, `f=32` = RGBA
- `i=<id>` — image ID (u32)
- `m=1` — more chunks follow; `m=0` — final chunk
- `q=1` — suppress OK response; `q=2` — suppress errors
- `c=<cols>`, `r=<rows>` — display dimensions in terminal cells

Chunking: each chunk is ≤ 4096 bytes of base64 (must be multiple of 4 bytes, except the last chunk). The full PNG can be sent in one sequence if it fits, or in multiple.

Minimal sequence for a PNG:
```
ESC _ G a=T,f=100,q=1; <entire-base64-encoded-PNG> ESC \
```

### Critical Problem: vte Drops APC Sequences

The vte crate (version 0.15, used in arcterm-vt) silently discards all APC sequences. In the state machine, `ESC 0x5F` triggers transition to `SosPmApcString`, where all subsequent bytes route through `anywhere()` which only processes terminal-control bytes (0x18, 0x1A, 0x1B). There is no callback or dispatch for APC data.

This means the standard Kitty graphics protocol cannot be received via the vte parser without modification.

### Three Options for APC Handling

**Option A: Pre-processor layer (byte-stream scan before vte)**
Scan the raw PTY byte stream before passing to `vte::Parser::advance()`. When `ESC _` is detected, consume bytes until `ESC \`, extract the payload, and emit a synthetic callback without forwarding those bytes to vte. The remaining bytes are forwarded normally.

- Complexity: Moderate. Requires a stateful byte scanner that handles partial reads across `advance()` calls (since PTY reads are chunked arbitrarily).
- Correctness risk: The scanner must handle `ESC \` appearing inside base64 payload — it cannot, because `ESC \` is the standard ST (String Terminator) that ends APC. Base64 does not produce ESC bytes, so this is safe.
- Impact on other features: None — vte sees clean bytes without APC sequences.

**Option B: Fork or patch vte to add APC dispatch**
Add an `apc_dispatch` method to the `Perform` trait and route `SosPmApcString` state to it.

- Complexity: High. Requires maintaining a fork of vte. The vte project has not accepted APC dispatch PRs historically (Alacritty does not support Kitty graphics).
- Risk: Fork divergence from upstream security/bug fixes.

**Option C: Replace vte with a different parser**
Use a parser that supports APC (e.g., a custom state machine, or the `vte` parser from WezTerm which has more complete parsing).

- Complexity: Very high for Phase 4. The existing codebase has deep vte integration.

**Recommendation: Option A (pre-processor layer)**

A pre-processor byte scanner is the correct approach. It is self-contained, does not touch the vte integration, and can be implemented as a small struct in `arcterm-vt` that wraps the existing `Processor`. The scanner maintains a small state machine (`Normal | InApc { buf: Vec<u8> }`) and calls a `kitty_graphics_command(&str)` handler callback when an APC sequence is fully received.

### Image Decoding

For PNG and JPEG decoding from bytes, two options:

| Criteria | image crate 0.25.10 | png + jpeg-decoder separately |
|---|---|---|
| Downloads (recent) | 18.3M | 21.7M (png), 7.3M (jpeg-decoder) |
| License | MIT OR Apache-2.0 | MIT OR Apache-2.0 |
| Last updated | Mar 2026 | Feb 2026 (png), Jun 2025 (jpeg) |
| API | `ImageReader::new(Cursor::new(bytes)).with_guessed_format()?.decode()? → DynamicImage` | Separate decode steps |
| Output | `DynamicImage` → `.to_rgba8()` → `RgbaImage` → `&[u8]` | Raw pixel Vec<u8> |
| Binary size | Larger (multi-format) | Smaller (only two formats) |
| Maintenance | Single crate, very active | Two crates to track |

The `image` crate is simpler: one call chain produces an RGBA pixel buffer. The `DynamicImage::to_rgba8()` method produces an `RgbaImage` whose underlying `Vec<u8>` is the raw RGBA pixel data needed for wgpu texture upload.

**Recommendation: image crate with default PNG + JPEG features**

Enable only needed formats:
```toml
image = { version = "0.25", default-features = false, features = ["png", "jpeg"] }
```

### wgpu Texture Creation from Image Bytes

The existing `GpuState` exposes `device` and `queue`. Creating an image texture follows this pattern (no new wgpu API needed — the project already uses wgpu 28):

1. `device.create_texture(&TextureDescriptor { size: Extent3d { width, height, depth_or_array_layers: 1 }, format: TextureFormat::Rgba8UnormSrgb, usage: TEXTURE_BINDING | COPY_DST, ... })`
2. `queue.write_texture(TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: Origin3d::ZERO, aspect: TextureAspect::All }, &rgba_bytes, TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * width), rows_per_image: None }, Extent3d { width, height, ... })`
3. `tex.create_view(&TextureViewDescriptor::default())`

The `bytes_per_row` must be a multiple of 256 for wgpu's alignment requirement. Calculation: `((4 * width) + 255) & !255`.

To render the texture quad, a new `ImageQuadRenderer` (separate from `QuadRenderer`) must be built with a textured pipeline (vertex + UV coordinates, sampler, texture bind group). The existing `QuadRenderer` is solid-color only and cannot be extended for textured quads without a separate pipeline.

---

## Section 5: JSON Rendering

No external crate is needed for JSON pretty-printing. The workspace already depends on `serde_json 1.0.149` (via rmpv). The approach:
- Parse with `serde_json::from_str::<serde_json::Value>(&content)` — validates JSON and builds a tree
- Pretty-print with `serde_json::to_string_pretty(&value)` — produces indented output
- Apply color by string-scanning the pretty output: strings → one color, numbers → another, keys → another, booleans/null → another
- Submit as a sequence of colored glyphon spans using `shape_row_into_buffer` pattern

For Phase 4 (no collapse/expand), this is sufficient. A full tree-widget renderer is deferred.

---

## Section 6: Architecture Decision — Where Structured Blocks Live

### Option A: Structured blocks embedded in the Grid (special cells/rows referencing a StructuredBlock)

Cells in the Grid gain a `block_id: Option<u32>` field, pointing to a `StructuredBlock` stored in a `HashMap<u32, StructuredBlock>` on the Grid. Rows spanning a structured block contain a sentinel cell type.

**Strengths:**
- Scrollback naturally carries structured blocks (blocks are part of the row history)
- Viewport rendering (`rows_for_viewport`) already works per-row — blocks participate automatically
- Block position is stable relative to terminal output

**Weaknesses:**
- `Cell` currently is 16–32 bytes; adding `block_id: Option<u32>` bloats every cell in the grid (80 cols × 10,000 scrollback rows = 800,000 cells, each storing a field that is None 99.9% of the time)
- The Grid's `cells: Vec<Vec<Cell>>` model assumes uniform cell geometry — structured blocks have variable pixel heights
- `rows_for_viewport` returns `&[&Vec<Cell>]` which the renderer iterates row-by-row; rendering a block requires the renderer to recognize sentinel cells and switch rendering mode

**Assessment:** Invasive change to the hottest data structure. Cell bloat is a real concern for the 10,000-row scrollback buffer. Requires coordinated changes across `arcterm-core` and `arcterm-render`.

### Option B: Overlay layer separate from the Grid (like selection quads and OverlayQuad)

Structured blocks are stored as a separate `Vec<StructuredBlock>` per pane, indexed by (scrollback_row, end_row) coordinates. The renderer receives both the Grid and the block list, renders the Grid's text/cells normally, then overlays blocks as positioned wgpu draws.

**Strengths:**
- Zero impact on Grid or Cell — no changes to `arcterm-core`
- The existing `render_multipane` already handles overlay quads (`overlay_quads: &[OverlayQuad]`) — this extends the same pattern
- Blocks can have variable pixel height without affecting the grid geometry
- Clean separation: the VT parser populates the block list; the renderer consumes it

**Weaknesses:**
- Block positions are expressed in (row, col) grid coordinates that must be converted to pixel positions at render time — the conversion is straightforward but requires cell_size knowledge
- Scrollback requires the block list to also track scrollback-relative positions, otherwise blocks "float" incorrectly during scroll
- Copy button hit-testing requires knowing the block's pixel rect — this is derivable from (row, col) + cell_size

**Assessment:** Preferred for Phase 4. It is additive: no existing data structure changes, no Cell bloat, no breaking changes to `arcterm-core`. The `render_multipane` pattern already supports overlay rendering.

### Option C: Replace Grid rows with a content stream model (StructuredBlock | PlainRows)

The terminal grid's `cells: Vec<Vec<Cell>>` becomes a `ContentStream: Vec<ContentItem>` where `ContentItem` is an enum of `PlainRow(Vec<Cell>)` or `StructuredBlock(...)`.

**Strengths:**
- Most accurate model — blocks participate naturally in the content stream
- Scrollback is unified — blocks scroll with content

**Weaknesses:**
- Requires rewriting `Grid`, `rows_for_viewport`, the entire rendering path, scrollback logic, and VT cursor handling — every row operation now touches a match arm
- `rows_for_viewport` no longer returns a uniform slice — the renderer must handle two types
- This is a Phase 7+ change, not a Phase 4 change
- Breaks the entire existing test suite for arcterm-core

**Assessment:** Architecturally correct long-term, but wrong for Phase 4. The scope would expand to a full grid rewrite.

---

## Comparison Matrix: All External Crates

| Criteria | syntect 5.3.0 | pulldown-cmark 0.13.1 | image 0.25.10 | regex 1.12.3 | serde_json 1.0.149 |
|---|---|---|---|---|---|
| Maturity | v5.x, 3 yrs major | v0.13, actively released | v0.25, ~8 yrs | v1.12, ~9 yrs | v1.0, ~8 yrs |
| Recent downloads | 2.9M | 17.2M | 18.3M | 113M | 127M |
| License | MIT | MIT | MIT OR Apache-2.0 | MIT OR Apache-2.0 | MIT OR Apache-2.0 |
| Last release | Sep 2025 | Feb 2026 | Mar 2026 | Feb 2026 | Jan 2026 |
| Pure Rust | Yes (default-fancy) | Yes | Yes | Yes | Yes |
| C bindings risk | Default config uses onig (C); avoidable | None | None | None | None |
| Key new dependency | fancy-regex (if default-fancy) | none new | none new | none new | already in workspace |
| Stack compatibility | Spans → glyphon Attrs (direct) | Events → spans (one pass) | RGBA bytes → wgpu texture | Feature flag → pane mode | Value → pretty-print string |
| Already in workspace | No | No | No | No | Yes (via rmpv) |

---

## Recommendation

### Syntax Highlighting
**Selected: syntect 5.3.0, `default-fancy` feature**

Use `SyntaxSet::load_defaults_newlines()` and `ThemeSet::load_defaults()`. The `Style.foreground` RGBA values map directly to `glyphon::Color::rgb(r, g, b)`, requiring zero intermediate translation. The `default-fancy` feature eliminates C bindings (no onig). Each syntax dump is ~200 KB. tree-sitter is not chosen because: (a) no bundled grammars or themes, (b) each grammar is a separate crate, (c) highlight query files require authoring and shipping, (d) significantly higher integration complexity for no Phase 4 benefit (semantic awareness is deferred).

### Markdown Rendering
**Selected: pulldown-cmark 0.13.1**

The pull-parser event model maps naturally to the span-building approach already used in `shape_row_into_buffer`. Phase 4 needs only: headings (larger line height via glyphon Metrics), lists (prefix bullet character), inline code (monospace + background tint quad), and bold/italic (Attrs weight/style). comrak adds AST allocation overhead without benefit for display-only rendering.

### Image Decoding
**Selected: image 0.25.10, `png` + `jpeg` features only**

One call chain (`to_rgba8()`) produces the exact pixel buffer format needed for `queue.write_texture`. Splitting into `png` + `jpeg-decoder` crates introduces two maintenance surfaces. `serde_json` is already in the workspace.

### APC / Kitty Graphics Handling
**Selected: Pre-processor byte scanner in arcterm-vt**

A small `ApcScanner` struct wraps the existing `Processor`. It maintains state `{ Normal, InApc { buf: Vec<u8> } }` and scans each byte from the PTY read before forwarding to `Processor::advance`. When `ESC _` is detected, it begins buffering. When `ESC \` is detected, it calls `handler.kitty_graphics_command(&buf)` and clears state. Non-APC bytes are forwarded unmodified. This approach requires zero changes to vte and is isolated to a single new struct in `arcterm-vt`.

### Architecture: Structured Block Storage
**Selected: Option B (overlay layer separate from Grid)**

Structured blocks are stored as `Vec<StructuredBlock>` per pane alongside the Grid, not inside it. The VT parser populates the list during OSC 7770 parsing. The renderer receives it alongside `PaneRenderInfo` and renders blocks as positioned overlays. This is additive: zero changes to `arcterm-core::Grid` or `arcterm-core::Cell`, no cell bloat, and it extends the existing `overlay_quads` pattern in `render_multipane`. Option A (blocks in Grid cells) is not chosen because it bloats every Cell in 10,000-row scrollback with an `Option<u32>` field that is None 99.9% of the time. Option C (ContentStream) is not chosen because it requires a full Grid rewrite — scope inappropriate for Phase 4.

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| syntect `default-fancy` compile time is slow (fancy-regex can be heavy) | Medium | Low | Benchmark on CI; if unacceptable, the `default-onig` feature (with onig C bindings) is a fallback. The `fancy-regex` crate is pure Rust but uses backtracking for lookaheads which can be slow to compile. |
| APC pre-processor fails on partial PTY reads spanning `ESC _` or `ESC \` across two `advance()` calls | High | High | The `ApcScanner` must be stateful across calls. The `InApc` state retains partial buffer across invocations. Add a unit test with PTY data split at every byte boundary around the APC sequence. |
| OSC 7770 content (large code block) is lost due to vte OSC params limit (MAX_OSC_PARAMS = 16) | Low | High | The content itself is in the raw OSC buffer (`osc_raw: Vec<u8>` with std feature), not as a param. Only the header params are delimited by `;` — the content after the start sequence arrives as ordinary PTY bytes. The 16-param limit applies to the header only, which uses ≤ 5 params. Verify with a test. |
| wgpu texture `bytes_per_row` alignment requirement (must be multiple of 256) causes panic for odd-width images | High | Medium | Always align: `bytes_per_row = (4 * width + 255) & !255`. Pad the pixel buffer with extra bytes if necessary, or use the image crate's `to_rgba8()` which produces a contiguous buffer that can be padded. |
| Image rendering pipeline (textured quad) conflicts with existing QuadRenderer (solid-color only) | Low | Medium | Implement `ImageQuadRenderer` as a separate struct alongside `QuadRenderer`. The render pass draws: solid quads → text → image quads (ordered to appear below text overlays). |
| Auto-detection false positives: a shell prompt containing `{` at line start triggers JSON detection | Medium | Medium | Require a valid `serde_json::from_str` parse before committing to JSON render mode. For code blocks, require both opening ` ``` ` and closing ` ``` ` markers before rendering. |
| pulldown-cmark emits multiple adjacent Text events; naive rendering misses text between events | Medium | Low | Use `TextMergeStream` wrapper (built into pulldown-cmark) to consolidate adjacent text events before processing. |
| syntect `SyntaxSet::load_defaults_newlines()` takes ~23ms on first call; causes frame drop | Low | Low | Call once at startup and store in `Renderer` or a global `Arc<SyntaxSet>`. Do not call per-frame or per-block. |
| Kitty image IDs collide across multiple images | Low | Low | Use a monotonically incrementing `u32` counter per pane. Wrap at u32::MAX (unlikely in practice). |

---

## Implementation Considerations

### Integration Points with Existing Code

**arcterm-vt/src/processor.rs (`Performer::osc_dispatch`)**
Add a `b"7770"` arm to the existing `match params[0]` block. Parse `params[1]` as `b"start"` or `b"end"`. Parse `params[2]` for `type=<value>` on start. Call `self.handler.structured_content_start(content_type, attrs)` or `self.handler.structured_content_end()`.

**arcterm-vt/src/processor.rs (new `ApcScanner` wrapper)**
A new `ApcScanner<H: Handler>` struct wraps `Processor` and `H`. Its `advance(handler, bytes)` method scans for `ESC _` / `ESC \`, routes APC content to `handler.kitty_graphics_command(...)`, and forwards non-APC bytes to `Processor::advance`.

**arcterm-vt/src/handler.rs (`Handler` trait)**
Add methods:
- `fn structured_content_start(&mut self, content_type: ContentType, attrs: &HashMap<&str, &str>) {}`
- `fn structured_content_end(&mut self, content: &str) {}`
- `fn kitty_graphics_command(&mut self, payload: &str) {}`

**arcterm-core/src/grid.rs**
No changes required for Option B architecture. The Grid stores only plain terminal cells as today.

**arcterm-render/src/renderer.rs (`render_multipane`)**
Extend `PaneRenderInfo` to include `structured_blocks: &[StructuredBlock]`. In `render_multipane`, after rendering cell backgrounds and text for a pane, iterate `structured_blocks` and call block-specific renderers positioned by `(block.start_row, 0)` converted to pixel coordinates via `cell_size`.

**arcterm-render/src/text.rs**
Add a `prepare_rich_text_block` method that accepts a `Vec<(String, glyphon::Attrs)>` and positions it at an absolute pixel offset. This reuses the existing `pane_buffer_pool` accumulation pattern.

**arcterm-render/src/quad.rs**
The existing `QuadRenderer` handles copy button backgrounds (solid-color quads). No changes to the solid-color pipeline are needed. A new `ImageQuadRenderer` (separate struct) handles textured quads for images.

### Migration Path

There is no existing solution to migrate. Phase 4 is purely additive: the OSC 7770 and APC paths are new code paths with no overlap with existing VT processing.

### Testing Strategy

1. **OSC 7770 parsing unit tests**: Feed raw byte sequences including `ESC ] 7770 ; start ; type=code_block ; lang=rust ST <content> ESC ] 7770 ; end ST` through `Processor::advance` with a mock `Handler`. Assert `structured_content_start` and `structured_content_end` fire correctly.
2. **APC scanner boundary tests**: Split a Kitty APC sequence at every possible byte boundary across two `advance()` calls. Assert the full payload is delivered.
3. **syntect rendering tests**: Highlight a known Rust snippet and assert the first span has a non-default foreground color (does not need to match an exact color — palette-dependent).
4. **JSON detection false-negative tests**: Feed `{ "key": "value" }` and assert JSON render is triggered. Feed `{foo}` (shell brace expansion) and assert JSON render is NOT triggered (parse fails).
5. **Auto-detection non-interference tests**: Feed standard `ls -la` output and assert no structured content is triggered.

### Performance Implications

- `SyntaxSet` and `ThemeSet` loading: ~23ms one-time cost. Load during `Renderer::new()`, not per-frame.
- syntect `highlight_line` per-line cost: sub-millisecond for typical lines (<200 chars). For a 50-line code block, total ≤ 5ms — acceptable for an event triggered by user-visible structured output, not every frame.
- Image texture upload: `queue.write_texture` is asynchronous on the GPU; CPU-side decode is the bottleneck. A 1920×1080 JPEG decode via the `image` crate is ~10–30ms. This should be done on a background Tokio task and the resulting `Vec<u8>` sent to the render thread.
- Structured block rendering: The glyphon atlas handles block text the same as terminal text. No performance regression expected.

---

## Sources

1. `https://crates.io/api/v1/crates/syntect` — version, downloads, license
2. `https://crates.io/api/v1/crates/pulldown-cmark` — version, downloads, license
3. `https://crates.io/api/v1/crates/image` — version, downloads, license
4. `https://crates.io/api/v1/crates/tree-sitter` — version, downloads
5. `https://crates.io/api/v1/crates/regex` — version, downloads, license
6. `https://crates.io/api/v1/crates/serde_json` — version, downloads
7. `https://crates.io/api/v1/crates/png` — version, downloads
8. `https://crates.io/api/v1/crates/jpeg-decoder` — version, downloads
9. `https://crates.io/api/v1/crates/base64` — version, downloads
10. `https://raw.githubusercontent.com/trishume/syntect/master/Cargo.toml` — feature flags, `default-fancy` configuration
11. `https://raw.githubusercontent.com/trishume/syntect/master/src/easy.rs` — `HighlightLines` API
12. `https://raw.githubusercontent.com/trishume/syntect/master/src/highlighting/style.rs` — `Style`, `Color`, `FontStyle` struct fields
13. `https://docs.rs/syntect/latest/syntect/parsing/struct.SyntaxSet.html` — `load_defaults_newlines()`, `load_defaults_nonewlines()`, ~200 KB per dump
14. `https://github.com/raphlinus/pulldown-cmark` — Event types, TextMergeStream, pull-parser pattern
15. `https://github.com/image-rs/image` — `ImageReader::new(Cursor::new(bytes)).with_guessed_format()?.decode()?`, `DynamicImage::to_rgba8()`
16. `https://sw.kovidgoyal.net/kitty/graphics-protocol/` — APC sequence format, chunking (m key), format codes (f=100 for PNG), action codes (a=T), response suppression (q=1)
17. `https://sw.kovidgoyal.net/kitty/graphics-protocol/#the-transmission-medium` — chunk size ≤ 4096 bytes base64, m=0/m=1 semantics
18. `https://sotrh.github.io/learn-wgpu/beginner/tutorial5-textures/` — wgpu Texture creation pattern, `bytes_per_row` alignment, `queue.write_texture` signature
19. `https://raw.githubusercontent.com/alacritty/vte/master/src/lib.rs` — APC state machine: `SosPmApcString` drops all data; OSC `std` feature uses unbounded Vec; MAX_OSC_PARAMS = 16
20. `https://raw.githubusercontent.com/alacritty/vte/master/Cargo.toml` — `std` feature is default; enables Vec-based OSC buffer
21. `https://docs.rs/vte/latest/vte/trait.Perform.html` — Perform trait methods: no APC dispatch; hook/put/unhook only for DCS
22. `https://crates.io/api/v1/crates/syntect/5.3.0/dependencies` — syntect dependency list (onig optional, fancy-regex optional)

---

## Uncertainty Flags

1. **syntect compile time with `default-fancy`**: fancy-regex is pure Rust but uses a backtracking engine for lookahead patterns in .tmLanguage grammars. Compile time impact on the arcterm workspace is unknown without measurement. If compile time is unacceptable, `default-onig` (C binding but faster compile) is the alternative.

2. **OSC 7770 content accumulation across multiple `advance()` calls**: The protocol places content between two OSC sequences as plain terminal characters. The exact behavior when an AI tool emits a very large code block (megabyte+) interleaved with cursor movement sequences is not prototyped. The assumption is that the VT parser processes characters normally between the start and end OSC sequences, and the handler buffers them. This needs a prototype to verify no chars are lost during cursor movements within the block.

3. **Kitty graphics terminal response handling**: If the terminal emits an `OK` response to the image sender (via the PTY write-back path), the client process receives it. For the `q=1` suppression approach, this is avoided. However, if a client program sends images without `q=1` and the response arrives as input to the process, behavior is undefined in arcterm's current pending_replies mechanism. This is low-priority for Phase 4 basic raster support but needs investigation if image-sending tools rely on the response.

4. **Image texture memory management**: wgpu textures allocated for images must be freed when the image scrolls out of the viewport or the pane is closed. The ownership model for `StructuredBlock` entries containing `wgpu::Texture` is not resolved — `wgpu::Texture` is not `Send` + `Sync` in all configurations, which may require Arc wrapping or a dedicated GPU-thread texture registry. This needs investigation before implementation begins.

5. **Structured block scroll coordinate stability**: When new lines are appended above a structured block (rare but possible in some shell output patterns), the block's `start_row` coordinate relative to the scrollback must remain correct. The overlay approach (Option B) requires that `StructuredBlock` stores absolute scrollback-relative row indices, not screen-relative. The mapping between scrollback rows and pixel positions at render time requires knowing `grid.scroll_offset` — this is available on the Grid but the coupling needs explicit design.
