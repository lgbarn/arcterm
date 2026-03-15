# Research: Phase 1 Foundation — Crate Versions, APIs, and Rendering Architecture

## Context

Arcterm is a Rust-native, GPU-accelerated terminal emulator. Phase 1 establishes every core
subsystem: window creation (winit), GPU rendering (wgpu), font rasterization (swash via
cosmic-text), PTY management (portable-pty), VT parsing (vte), and async coordination (tokio).
The design decisions in `.shipyard/phases/1/CONTEXT-1.md` are treated as fixed constraints for
this research; this document provides the version data, API patterns, and integration gotchas
needed to implement those decisions.

---

## Comparison Matrix — Crate Versions and Ecosystem Health

| Crate | Latest Stable | Release Cadence | License | Key Dependents | Maintenance Signal |
|-------|--------------|-----------------|---------|----------------|--------------------|
| `vte` | 0.15.0 | Infrequent (alacritty-driven) | MIT / Apache-2.0 | 19,700+ crates | Stable, low churn |
| `portable-pty` | 0.9.0 (2025-02-11) | Irregular, WezTerm-paced | MIT | WezTerm, others | Active, WezTerm-backed |
| `wgpu` | 28.0.0 | ~Quarterly major | MIT / Apache-2.0 | Bevy, Rio, Zed | Very active, breaking major versions |
| `winit` | 0.30.12 (2025-11-16) | Active | MIT / Apache-2.0 | Most wgpu apps | Active, 0.30 API is stable |
| `swash` | 0.2.6 | Slow (single author) | MIT / Apache-2.0 | cosmic-text | Low churn, 33% documented |
| `cosmic-text` | 0.18.2 (2025-02-20) | Active (Pop!_OS driven) | MIT | Iced, COSMIC DE | Very active, breaking changes frequent |
| `tokio` | 1.50.0 | Rapid; 1.47.x LTS | MIT | Ubiquitous | Extremely stable 1.x API |
| `glyphon` | 0.9.0 (2025-04-11) | Active | MIT / Apache-2.0 / zlib | ~1,900 crates | Active, tracks wgpu versions |

**Recommended pinned versions for `Cargo.toml`:**

```toml
vte          = "0.15"
portable-pty = "0.9"
wgpu         = "28"
winit        = "0.30"
swash        = "0.2"
cosmic-text  = "0.18"
tokio        = { version = "1", features = ["full"] }
glyphon      = "0.9"
```

---

## Detailed Analysis

### 1. `vte` — VT Parser

**Version:** 0.15.0
**Source:** https://github.com/alacritty/vte / https://docs.rs/vte/latest/vte/

**Core architecture:**
The crate implements Paul Williams' ANSI state machine. It does not assign semantic meaning
to sequences; that is entirely the caller's responsibility. The `Parser` struct holds the
state machine state. You call `parser.advance(&mut performer, bytes)` for each chunk of
bytes read from the PTY. `Parser` then calls the appropriate method on your `Perform`
implementation for each recognized sequence.

**Perform trait — all methods have default no-op implementations:**

```rust
pub trait Perform {
    // Printable Unicode character (already decoded from UTF-8 by the parser)
    fn print(&mut self, c: char) {}

    // C0 or C1 control byte (e.g., 0x08 = BS, 0x0A = LF, 0x0D = CR, 0x07 = BEL)
    fn execute(&mut self, byte: u8) {}

    // DCS sequence start — select handler for subsequent put() bytes
    fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {}

    // Byte within an active DCS string
    fn put(&mut self, byte: u8) {}

    // DCS string terminated
    fn unhook(&mut self) {}

    // OSC sequence (e.g., OSC 0 ; title BEL = set window title)
    // params: each semicolon-delimited segment as a byte slice
    // bell_terminated: true if 0x07 terminated the sequence (not ST)
    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {}

    // CSI sequence (e.g., ESC[31m, ESC[2J, ESC[H)
    // params: numeric parameters; intermediates: intermediate bytes; action: final byte
    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {}

    // ESC sequence that is not CSI/OSC/DCS
    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {}

    // Advanced: called by Parser::advance_until_terminated() to stop early
    fn terminated(&self) -> bool { false }
}
```

**Typical integration pattern (arcterm-vt crate):**

The standard approach used by Alacritty is a two-layer design:

1. A `Processor` struct (wraps `vte::Parser`) that implements `Perform` as a thin bridge.
2. A `Handler` trait (defined in arcterm-vt) with semantic methods like `set_cursor_position`,
   `set_color`, `erase_line`, `put_char`. The `Processor`'s `Perform` impl interprets raw
   sequences and calls `Handler` methods.
3. The `Grid` struct implements `Handler` and mutates terminal cell state.

```rust
// arcterm-vt data flow
PTY bytes
  -> vte::Parser::advance(&mut Processor, bytes)
  -> Processor impls Perform, interprets sequences
  -> calls Handler trait methods on &mut Grid
  -> Grid mutates cells, cursor, scrollback
  -> Renderer reads dirty cells from Grid each frame
```

**Gotchas:**
- `vte` only supports 7-bit escape codes. 8-bit C1 controls (0x80–0x9F) are not parsed as
  escape sequences; they arrive as raw bytes in `execute()`. Modern terminals rarely need 8-bit
  C1 codes, but be aware when handling legacy applications.
- `Params` is a compact type — do not attempt to store it; decode values during the callback.
- OSC sequences can be quite long (e.g., base64 encoded Kitty image data will arrive in chunks
  via DCS, not OSC). Phase 4 Kitty support will need the `hook`/`put`/`unhook` path.
- The `ignore` flag in `csi_dispatch` and `esc_dispatch` indicates the parser exceeded its
  parameter/intermediate buffer. Sequences with `ignore = true` should be treated as malformed
  and discarded rather than partially interpreted.

---

### 2. `portable-pty` — PTY Allocation and Shell Spawning

**Version:** 0.9.0 (2025-02-11)
**Source:** https://docs.rs/portable-pty/latest/portable_pty/

**Core API:**

```rust
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

// 1. Get the native PTY system (Unix: openpty/forkpty; Windows: ConPTY)
let pty_system = native_pty_system();

// 2. Open a PTY pair with initial size
let pair = pty_system.openpty(PtySize {
    rows: 24,
    cols: 80,
    pixel_width: 0,  // optional; used by some programs for DPI awareness
    pixel_height: 0,
}).unwrap();

// 3. Build the command
let mut cmd = CommandBuilder::new("bash");
cmd.env("TERM", "xterm-256color");

// 4. Spawn the child process attached to the slave side
let child = pair.slave.spawn_command(cmd).unwrap();
// slave side is no longer needed after spawn on Unix
drop(pair.slave);

// 5. Writer: sends keyboard input to the shell
let mut writer = pair.master.take_writer().unwrap();

// 6. Reader: receives shell output (blocking read)
let mut reader = pair.master.try_clone_reader().unwrap();

// 7. Resize
pair.master.resize(PtySize { rows: 40, cols: 120, pixel_width: 0, pixel_height: 0 }).unwrap();
```

**Async / tokio integration:**
`portable-pty`'s reader and writer are synchronous (`Read` + `Write`). The standard pattern
for use with tokio is to move the blocking reader into a dedicated OS thread spawned via
`tokio::task::spawn_blocking` (or a plain `std::thread::spawn`) and use a tokio channel to
forward data to the async task that feeds the VT parser.

```rust
// Recommended pattern: blocking reader in a dedicated thread
let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
std::thread::spawn(move || {
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,          // PTY closed (shell exited)
            Ok(n) => { let _ = tx.blocking_send(buf[..n].to_vec()); }
            Err(_) => break,
        }
    }
});

// In the tokio async context, receive and feed to the VT parser:
while let Some(bytes) = rx.recv().await {
    parser.advance(&mut grid, &bytes);
    // mark frame dirty, request redraw
}
```

**Gotchas:**
- On macOS, you must drop `pair.slave` after `spawn_command`; holding it prevents the shell
  from detecting EOF on exit.
- `try_clone_reader()` is the correct method (returns a cloned reader handle). `take_reader()`
  (if present) would consume the only reader.
- `portable-pty` does not integrate with tokio directly; no `AsyncRead` impl is provided.
  The `spawn_blocking` / channel approach above is the idiomatic pattern used by WezTerm
  internally.
- `pixel_width` / `pixel_height` in `PtySize` are passed to the kernel but support varies by
  OS. Set to 0 initially; add proper pixel dimension tracking in Phase 2 for applications that
  query `TIOCGWINSZ`.
- The crate handles Windows ConPTY transparently via the same API. No conditional compilation
  is needed in arcterm-pty.

---

### 3. `wgpu` — GPU Renderer

**Version:** 28.0.0
**Source:** https://wgpu.rs / https://docs.rs/wgpu/latest/wgpu/ / https://github.com/gfx-rs/wgpu

**wgpu releases break the API approximately every quarter.** The crate uses major version
bumps aggressively (e.g., v22 → v28 within ~18 months). Pin to `"28"` and treat upgrades as
deliberate migration tasks.

**Surface creation lifecycle (winit 0.30 integration):**

The critical constraint is that on macOS (and Android/iOS), the `wgpu::Surface` must be
created *after* the first `ApplicationHandler::resumed()` event fires. Attempting surface
creation during `new()` or before `resumed()` will panic on Metal.

The idiomatic pattern using `Arc<Window>` to satisfy lifetime requirements:

```rust
struct App {
    window: Option<Arc<Window>>,
    gpu: Option<GpuState>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop.create_window(Window::default_attributes()).unwrap()
        );
        // wgpu surface borrows the window handle; Arc makes the lifetime 'static
        let gpu = pollster::block_on(GpuState::new(Arc::clone(&window)));
        self.window = Some(window);
        self.gpu = Some(gpu);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop,
                    _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => { /* render */ }
            WindowEvent::Resized(size) => { /* reconfigure surface */ }
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => { /* send to PTY */ }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = &self.window { w.request_redraw(); }
    }
}
```

**Core initialization sequence inside `GpuState::new()`:**

```rust
async fn new(window: Arc<Window>) -> Self {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    let surface = instance.create_surface(window.clone()).unwrap();
    let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }).await.unwrap();
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor {
        label: None,
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::downlevel_webgl2_defaults()
            .using_resolution(adapter.limits()),
    }, None).await.unwrap();
    // configure surface
    let surface_caps = surface.get_capabilities(&adapter);
    let format = surface_caps.formats[0]; // prefer sRGB if available
    surface.configure(&device, &wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: window.inner_size().width,
        height: window.inner_size().height,
        present_mode: wgpu::PresentMode::Fifo, // vsync; change to Mailbox for lower latency
        alpha_mode: surface_caps.alpha_modes[0],
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    });
    // ...
}
```

**Gotchas:**
- wgpu 28 uses `wgpu::InstanceDescriptor` where older versions used `wgpu::Backends` directly.
  The API changes substantially between major versions; always check the changelog when upgrading.
- On macOS, `wgpu::Backends::METAL` is the native backend. `wgpu::Backends::all()` selects
  Metal automatically. Do not set `force_fallback_adapter = true` in production; it selects the
  software renderer.
- `PresentMode::Fifo` is vsync and the safest cross-platform default. Switch to `Mailbox` or
  `Immediate` for lower latency once you have latency measurement infrastructure (Phase 8).
- Reconfigure the surface on every `WindowEvent::Resized` — do not try to cache the old size.
  Failing to reconfigure causes a validation error on the next `get_current_texture()`.
- `pollster::block_on()` is needed to bridge the async wgpu init APIs inside the synchronous
  `ApplicationHandler::resumed()`. Add `pollster` to dependencies for this purpose.

---

### 4. `winit` — Window Management

**Version:** 0.30.12 (2025-11-16, latest stable)
**Source:** https://docs.rs/winit/0.30.12/winit/

**winit 0.30 introduced a breaking API change** from the old `EventLoop::run()` model to the
`ApplicationHandler` trait pattern. All wgpu tutorials predating late 2024 use the old API.
Ensure all referenced examples are for winit 0.30+.

**Key API changes in 0.30:**
- `Window` and `ActiveEventLoop` changed to traits (returning `Box<dyn Window>`).
- `EventLoopProxy::send_event` renamed to `EventLoopProxy::wake_up` (no longer carries a
  payload; use a shared queue instead for cross-thread wakeups).
- `ApplicationHandler` is the new required interface replacing the closure-based `run()`.
- Window creation must happen in `resumed()`, not at startup.

**Keyboard input for terminal use:**
Keyboard events arrive as `WindowEvent::KeyboardInput { event: KeyEvent, .. }`. The `KeyEvent`
struct contains:
- `physical_key: PhysicalKey` — hardware scancode, layout-independent
- `logical_key: Key` — layout-resolved key (e.g., `Key::Character("a")`)
- `text: Option<SmolStr>` — the actual string to send to the PTY (includes IME composition)
- `state: ElementState` — `Pressed` or `Released`

For terminal input, use `event.text` when present (this handles shift, AltGr, IME correctly).
Fall back to mapping physical keys for control sequences (Ctrl+C, arrow keys, F-keys) that do
not produce `text`.

**Gotchas:**
- macOS does not fire `resumed()` on startup in the same way iOS does; it fires once when the
  `EventLoop::run_app()` begins. It is safe to treat it as "application started" on desktop.
- winit 0.30.13 documentation failed to build on docs.rs; the stable API reference is available
  at version 0.30.12 or in the GitHub source.
- `about_to_wait()` replaces the old `MainEventsCleared` event. Call `window.request_redraw()`
  here for a continuous render loop, or only on state changes for event-driven rendering.
  Terminal output is bursty; prefer event-driven rendering (request redraw when PTY data
  arrives or cursor blinks) over a fixed-rate loop.

---

### 5. `cosmic-text` + `swash` — Text Shaping, Layout, and Rasterization

**cosmic-text version:** 0.18.2 (2025-02-20)
**swash version:** 0.2.6
**Source:** https://github.com/pop-os/cosmic-text / https://docs.rs/cosmic-text

**Architecture overview:**
`cosmic-text` layers on top of `swash` (rasterization) and `harfrust` (shaping). For a
terminal renderer, the hierarchy is:

```
cosmic-text / FontSystem  ← font discovery, fallback chains
cosmic-text / Buffer      ← text shaping and layout per text region
cosmic-text / SwashCache  ← glyph rasterization cache (wraps swash)
glyphon                   ← packs SwashCache output into a GPU texture atlas, renders via wgpu
```

You will interact with swash indirectly through `SwashCache`. Direct swash use is only needed
if you build a fully custom rasterization pipeline (not recommended for Phase 1).

**FontSystem — one per application:**

```rust
let mut font_system = cosmic_text::FontSystem::new();
// FontSystem auto-detects system fonts on creation.
// To add a custom font from bytes:
font_system.db_mut().load_font_data(font_bytes.to_vec());
```

**Buffer — one per text region (terminal viewport):**

For a terminal, the entire visible grid can be one large Buffer, or you can use one Buffer per
row. The single-buffer approach simplifies selection highlighting later. Use `Wrap::None` to
prevent cosmic-text from reflowing terminal lines:

```rust
let metrics = cosmic_text::Metrics::new(font_size, line_height); // e.g., 16.0, 20.0
let mut buffer = cosmic_text::Buffer::new(&mut font_system, metrics);
buffer.set_size(&mut font_system, Some(viewport_width), Some(viewport_height));
```

**Measuring cell width (critical for monospace grid):**
Shape a single "M" or "0" character and measure its advance to establish the cell width. Do
this once at font size initialization:

```rust
let attrs = cosmic_text::Attrs::new().family(cosmic_text::Family::Monospace);
let mut measure_buf = cosmic_text::Buffer::new(&mut font_system, metrics);
measure_buf.set_text(&mut font_system, "M", attrs, cosmic_text::Shaping::Basic);
measure_buf.shape_until_scroll(&mut font_system, false);
let cell_width = measure_buf
    .layout_runs()
    .next()
    .and_then(|run| run.glyphs.first())
    .map(|g| g.w)
    .unwrap_or(font_size);
```

**Disabling ligatures** (important for terminal fidelity — `fi`, `fl`, `->` must not merge):

The `harfrust::Feature` API controls OpenType features. Set `liga` and `calt` to disabled:

```rust
// Note: as of cosmic-text 0.17+, Attrs::matches was removed.
// Font features are set via the Attrs builder.
// The exact API for per-feature control may require checking the 0.18 source;
// in the absence of a clear API, choose a font with ligatures disabled by default
// (most monospace terminal fonts — JetBrains Mono, Cascadia Mono — ship a
// "no-ligatures" variant, or disable via font features in font selection).
```

**UNCERTAINTY:** The exact `Attrs` API for disabling individual OpenType features (liga, calt)
changed in cosmic-text 0.17.0 (which removed `Attrs::matches`). The `font_features` field
shown in older examples may not map exactly to 0.18. Verify against the 0.18.x source before
implementing. Using a ligature-free font variant is the safest Phase 1 fallback.

**SwashCache — glyph rasterization:**

`SwashCache` is used by `glyphon` internally. You only interact with it directly if building
a custom renderer. The `glyphon`-based approach (recommended below) handles this automatically.

**Breaking changes in cosmic-text to watch:**
- 0.17.0 removed `Attrs::matches`
- 0.16.0 introduced the `Renderer` trait (changes how `Buffer::draw()` works)
- 0.18.0 added ellipsis support — no breaking changes for terminal use

Pin to `"0.18"` and treat minor version bumps as potentially breaking.

---

### 6. `glyphon` — wgpu Text Renderer (Recommended for Phase 1)

**Version:** 0.9.0 (2025-04-11; requires Rust 1.92+)
**Source:** https://github.com/grovesNL/glyphon / https://docs.rs/glyphon
**License:** MIT / Apache-2.0 / zlib (triple)

**Why glyphon over a custom atlas:**
Building a custom glyph atlas from scratch requires: a 2D bin-packing algorithm (etagere),
LRU eviction logic, WGSL shader authorship for glyph sampling, subpixel rendering offsets,
and atlas texture management. `glyphon` provides all of this already, integrates directly with
`cosmic-text` (which is already in the stack), and follows wgpu's middleware render-pass
pattern. The Rio terminal built a custom renderer (`sugarloaf`) to gain fine-grained control
over terminal-specific rendering; this is appropriate for Phase 2+ optimization but is
premature for Phase 1.

**Architecture (from DeepWiki analysis):**

```
CPU side (per frame):
  1. text_renderer.prepare(device, queue, font_system, atlas, viewport, text_areas, swash_cache)
     a. cosmic-text Buffer provides shaped LayoutGlyph data
     b. SwashCache rasterizes glyphs on demand (calls swash internally)
     c. etagere packs new glyphs into the GPU texture atlas (LRU eviction when full)
     d. GlyphToRender vertex data (position, UV, color) uploaded to GPU buffer

GPU side (per frame):
  2. text_renderer.render(atlas, viewport, render_pass)
     a. Sets custom WGSL pipeline (bind group 0 = atlas texture + sampler)
     b. Issues draw calls within the caller's render pass — no extra render pass
  3. atlas.trim()  // evicts LRU glyphs to reclaim texture memory
```

**Initialization and per-frame render loop:**

```rust
// One-time setup
let cache = glyphon::Cache::new(&device);
let viewport = glyphon::Viewport::new(&device, &cache);
let mut atlas = glyphon::TextAtlas::new(&device, &queue, &cache, surface_format);
let mut text_renderer = glyphon::TextRenderer::new(&mut atlas, &device,
    wgpu::MultisampleState::default(), None);
let mut font_system = glyphon::FontSystem::new(); // re-exports cosmic_text::FontSystem
let mut swash_cache = glyphon::SwashCache::new();

// Per-frame
viewport.update(&queue, glyphon::Resolution {
    width: surface_width,
    height: surface_height,
});

text_renderer.prepare(
    &device, &queue, &mut font_system, &mut atlas, &viewport,
    &[glyphon::TextArea {
        buffer: &text_buffer,        // cosmic_text::Buffer
        left: 0.0,
        top: 0.0,
        scale: 1.0,
        bounds: glyphon::TextBounds {
            left: 0, top: 0,
            right: surface_width as i32,
            bottom: surface_height as i32,
        },
        default_color: glyphon::Color::rgb(0xFF, 0xFF, 0xFF),
        custom_glyphs: &[],
    }],
    &mut swash_cache,
).unwrap();

// Inside render pass:
text_renderer.render(&atlas, &viewport, &mut render_pass).unwrap();
atlas.trim();
```

**Gotchas:**
- `glyphon` re-exports `cosmic_text` types. Use `glyphon::FontSystem`, `glyphon::Buffer`,
  etc., rather than importing `cosmic_text` separately, to avoid version mismatches.
- `glyphon 0.9` requires Rust 1.92+. Ensure the CI toolchain is pinned appropriately.
- The `atlas.trim()` call after `render()` is not optional for long-running applications;
  omitting it will cause the atlas texture to grow unboundedly.
- `TextArea::scale` should match the window's DPI scale factor. Obtain it from
  `window.scale_factor()` and apply it to font metrics and glyph rendering.
- Terminal rendering differs from document rendering: the grid has a fixed number of cells,
  all with the same monospace font. Consider constructing the cosmic-text Buffer as a sequence
  of fixed-width spans (one per cell row) rather than one large reflowing text region.

---

### 7. Glyph Atlas Architecture — Recommended Approach

Based on research into Warp's implementation, Rio's sugarloaf, the glyphon codebase, and
Windows Terminal's Atlas Engine:

**For Phase 1: Use glyphon directly.** Do not build a custom atlas.

**For Phase 2+ optimization:** Consider a terminal-specific custom atlas with these properties
(documented here for future reference):

- **Glyph-level cache, not line-level.** In a monospace terminal, the same ~100 ASCII glyphs
  appear thousands of times. A glyph cache avoids redundant rasterization. A line cache would
  duplicate the same glyph bitmaps across thousands of cached lines.

- **Subpixel quantization.** Rather than caching one rasterized image per glyph, cache up to
  4 subpixel-offset variants (0.0, 0.25, 0.5, 0.75 pixel offsets). This keeps text visually
  crisp without unbounded atlas growth. Warp uses 3 subpixel bins (0.0, 0.33, 0.66).
  `cosmic-text`'s `SwashCache` uses a 4x4 `SubpixelBin` grid internally.

- **Two-texture atlas (color + mask).** Color emoji and bitmap fonts need RGBA texture storage;
  greyscale/LCD-antialiased glyphs use single-channel alpha masks. Separate atlas textures
  by content type to keep shader logic simple and avoid format waste.

- **LRU eviction.** For a terminal displaying 80x24 = 1,920 cells, the active glyph set is
  small (< 300 distinct glyphs in typical use). An LRU cache of 512 slots with a texture
  resolution of 2048x2048 pixels is more than sufficient.

- **Dirty-rectangle tracking.** The terminal grid model should track which cells changed since
  the last frame. Only re-upload vertex data for dirty cells, not the entire grid. This is the
  single largest rendering optimization for terminals with bursty output.

---

### 8. `tokio` — Async Runtime

**Version:** 1.50.0 (latest); 1.47.x is LTS until September 2026
**Source:** https://tokio.rs / https://github.com/tokio-rs/tokio

**Recommended features:** `tokio = { version = "1", features = ["full"] }` for Phase 1.
Trim unused features in Phase 8 optimization.

**Relevant patterns for arcterm:**

```rust
#[tokio::main]
async fn main() {
    // Launch the winit event loop on the main thread (required by macOS).
    // The tokio runtime handles PTY reading, VT parsing, and event dispatch
    // on background threads/tasks.

    // PTY reader: spawn_blocking bridges the synchronous portable-pty reader
    let handle = tokio::task::spawn_blocking(move || {
        // ... blocking read loop, sends to channel
    });

    // The winit event loop must run on the main thread:
    let event_loop = EventLoop::new().unwrap();
    event_loop.run_app(&mut app).unwrap();
}
```

**Important:** winit's event loop on macOS **must** run on the main thread. You cannot run
tokio's `#[tokio::main]` multi-threaded runtime and then move the event loop to another thread.
The correct structure is:

```rust
fn main() {
    // Initialize tokio runtime manually for background tasks
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    // Background tasks can be spawned here using rt.spawn(...)

    // Event loop runs on main thread — mandatory on macOS
    let event_loop = EventLoop::new().unwrap();
    event_loop.run_app(&mut app).unwrap();
}
```

**Tokio 1.x is stable.** The API has not had breaking changes since 1.0. The 1.47.x LTS
branch receives backported fixes until September 2026.

---

### 9. CI Considerations — wgpu on GitHub Actions

**The core problem:** wgpu requires a GPU or software renderer. GitHub Actions runners
(ubuntu-latest, windows-latest, macos-latest) do not have discrete GPUs. Tests that invoke
the wgpu pipeline (surface creation, texture upload, render passes) will fail on headless
Ubuntu unless a software renderer is configured.

**Recommended CI strategy for Phase 1:**

Split CI into two job categories:

**Category A — Non-GPU jobs (all platforms):**
- `cargo build`, `cargo test` (logic only, no wgpu surface), `cargo clippy`, `cargo fmt`
- Run on `ubuntu-latest`, `windows-latest`, `macos-latest`
- No special configuration needed
- Design the arcterm codebase so that VT parsing, PTY logic, and grid model are testable
  without instantiating wgpu. This is enforced by the multi-crate workspace structure
  (`arcterm-vt`, `arcterm-pty`, `arcterm-core` have no wgpu dependency).

**Category B — GPU rendering jobs (wgpu surface tests):**
On Linux (Ubuntu), use Mesa's software Vulkan renderer (lavapipe) or Mesa's GL renderer
(llvmpipe via OpenGL backend):

```yaml
# .github/workflows/ci.yml excerpt
jobs:
  test-linux-gpu:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install Mesa software rendering
        run: |
          sudo apt-get update
          sudo apt-get install -y libvulkan1 mesa-vulkan-drivers
      - name: Run wgpu rendering tests
        env:
          WGPU_BACKEND: gl          # use GLES via Mesa llvmpipe
          # OR: WGPU_BACKEND: vulkan  (lavapipe, requires mesa-vulkan-drivers)
        run: cargo test --package arcterm-render
```

On Windows CI, D3D12's **WARP** software adapter is available out of the box — wgpu
automatically falls back to WARP when no GPU is present. No extra configuration is needed for
`windows-latest`.

On macOS CI (`macos-latest`), GitHub's macOS runners include Metal GPU access. wgpu on macOS
will use Metal without software rendering.

**Recommendation:** Separate `cargo test` (logic, no GPU) from GPU integration tests.
Keep GPU tests in a separate job that only runs on `ubuntu-latest` with Mesa and on
`windows-latest` with WARP. Gate GPU tests with a `#[cfg(feature = "gpu-tests")]` feature
flag so they are opt-in and clearly labeled.

**Known issue:** Mesa lavapipe versions on `ubuntu-latest` vary day-to-day (pulled from a
PPA), which can cause intermittent failures. Pin the Mesa version or add a test-stability
retry policy for GPU jobs.

---

## Recommendation

**Selected approach: glyphon + cosmic-text for Phase 1; custom atlas deferred to Phase 2.**

| Decision | Choice | Rationale |
|----------|--------|-----------|
| VT parsing | `vte` 0.15 | Pre-decided; confirmed correct — battle-tested, 19K dependents |
| PTY | `portable-pty` 0.9 | Pre-decided; confirmed correct — WezTerm-backed, ConPTY on Windows |
| GPU API | `wgpu` 28 | Pre-decided; confirmed correct — Metal/Vulkan/DX12 from one API |
| Window | `winit` 0.30 | Pre-decided; use 0.30.12 specifically, not 0.31 beta |
| Text rendering | `glyphon` 0.9 | Provides glyph atlas, LRU eviction, WGSL shaders, cosmic-text integration out of the box. Building a custom atlas in Phase 1 would consume 2-4x the implementation time for no Phase 1 benefit. |
| Font shaping | `cosmic-text` 0.18 | Accessed via `glyphon` re-exports; handles system font discovery, fallback chains, HarfRust shaping |
| Font rasterization | `swash` 0.2.6 | Used indirectly through `cosmic-text`'s `SwashCache`; no direct API calls needed in Phase 1 |
| Async | `tokio` 1.50 | Pre-decided; confirmed correct — PTY reader in `spawn_blocking`, channels for PTY→VT pipeline |

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| wgpu major version bump breaks API before Phase 1 ships | Medium | Medium | Pin to `wgpu = "28"` exactly; treat upgrades as explicit migrations |
| cosmic-text 0.18 → 0.19 breaking change during development | High | Low | Pin `cosmic-text = "0.18"`; glyphon pins cosmic-text internally, so version must match glyphon's dependency |
| winit event loop threading conflict with tokio on macOS | Medium | High | Always run EventLoop on main thread; use rt.enter() pattern documented above; never move event loop to a spawned thread |
| Mesa lavapipe CI flakiness on ubuntu-latest | Medium | Low | Gate GPU render tests behind feature flag; add `continue-on-error: true` to GPU job in Phase 1; harden in Phase 2 |
| swash maintenance risk (single primary author, 33% docs) | Low | Medium | Using swash via glyphon/cosmic-text, not directly; if swash stalls, cosmic-text can swap its rasterization backend |
| Ligature disabling API unclear in cosmic-text 0.18 | Medium | Low | Use a monospace font without ligatures (Cascadia Mono, JetBrains Mono NL) as Phase 1 default; implement feature flag disable in Phase 2 |
| portable-pty 0.9 has no tokio integration | Low | Low | Use spawn_blocking + mpsc channel pattern; this is the established WezTerm pattern |
| DPI scaling errors in glyphon (mismatched scale_factor) | Medium | Medium | Always pass `window.scale_factor()` to font metrics and viewport resolution; test on HiDPI and 1x displays from day 1 |

---

## Implementation Considerations

### Integration Points in the Multi-Crate Workspace

```
arcterm-vt
  deps: vte
  exports: Grid, Handler trait, Processor

arcterm-pty
  deps: portable-pty, tokio
  exports: PtySession (owns master, writer, reader thread, output channel)

arcterm-core
  deps: (none — shared types only)
  exports: Cell, Color, GridSize, InputEvent, RenderFrame

arcterm-render
  deps: wgpu, winit, glyphon, cosmic-text, tokio
  exports: Renderer, GlyphCache (wraps glyphon), SurfaceManager

arcterm-app
  deps: arcterm-vt, arcterm-pty, arcterm-core, arcterm-render, tokio
  exports: main binary, ApplicationHandler impl
```

### PTY → VT → Render Pipeline

```
PtySession::reader_thread
  reads bytes from portable-pty master (blocking)
  sends Vec<u8> on tokio::sync::mpsc::Sender

arcterm-app event loop (tokio runtime)
  receives Vec<u8> from channel
  calls vte::Parser::advance(&mut grid_performer, &bytes)
  marks grid dirty, calls window.request_redraw()

winit RedrawRequested event
  arcterm-render::Renderer::render_frame(&grid)
  glyphon prepares and renders visible cell range
  wgpu presents frame
```

### Testing Strategy

- `arcterm-vt`: Unit test `Perform` implementation with crafted byte sequences (no GPU).
  Compare resulting grid state against expected cell contents and attributes.
- `arcterm-pty`: Integration test shell spawning and I/O on the CI platform (no GPU).
- `arcterm-render`: GPU tests gated behind `--features gpu-tests`; run only on CI with
  software renderer configured.
- `arcterm-app`: End-to-end test deferred to Phase 2 once the basic rendering pipeline
  is stable.

### Performance Implications

- The `glyphon` prepare step is CPU-bound (shaping + rasterization). For a 80x24 terminal,
  on the first frame all ~1,920 cells are shaped and rasterized; subsequent frames only process
  changed cells if dirty tracking is implemented in the grid. Implement dirty tracking in
  `arcterm-core::Grid` from the start.
- The wgpu render pass for a 1080p terminal window requires one draw call per glyph in the
  worst case. `glyphon` batches all glyphs into a single indexed draw call. This is adequate
  for Phase 1 and will not be the bottleneck.
- Key-to-screen latency budget: keyboard event → PTY write (~0.1ms) → shell echo → PTY read
  → VT parse → grid update → request_redraw → RedrawRequested → render → present. The wgpu
  present call with `PresentMode::Fifo` adds up to 16ms at 60Hz. Switch to `Mailbox` or
  `Immediate` when measuring latency in Phase 8.

---

## Sources

1. https://docs.rs/vte/latest/vte/ — vte 0.15.0 documentation
2. https://docs.rs/vte/latest/vte/trait.Perform.html — Perform trait method signatures
3. https://github.com/alacritty/vte — vte source repository
4. https://deepwiki.com/alacritty/vte — vte integration pattern analysis
5. https://docs.rs/portable-pty/latest/portable_pty/ — portable-pty 0.9.0 API
6. https://crates.io/crates/portable-pty — version and release date (0.9.0, 2025-02-11)
7. https://docs.rs/wgpu/latest/wgpu/ — wgpu 28.0.0 documentation
8. https://github.com/gfx-rs/wgpu — wgpu repository and changelog
9. https://docs.rs/winit/0.30.12/winit/ — winit 0.30.12 documentation
10. https://github.com/rust-windowing/winit — winit changelog
11. https://github.com/rust-windowing/winit/discussions/3667 — winit 0.30 + wgpu integration discussion
12. https://github.com/gfx-rs/wgpu/discussions/6005 — Arc<Window> lifetime pattern for wgpu surface
13. https://docs.rs/swash/latest/swash/ — swash 0.2.6 documentation
14. https://docs.rs/swash/latest/swash/scale/index.html — swash ScaleContext and rasterization API
15. https://docs.rs/cosmic-text/latest/cosmic_text/ — cosmic-text API
16. https://github.com/pop-os/cosmic-text/releases — cosmic-text 0.18.2 release history
17. https://deepwiki.com/pop-os/cosmic-text — cosmic-text architecture analysis
18. https://github.com/grovesNL/glyphon — glyphon 0.9.0 repository
19. https://deepwiki.com/grovesNL/glyphon — glyphon architecture analysis
20. https://docs.rs/glyphon/latest/glyphon/ — glyphon API documentation
21. https://github.com/tokio-rs/tokio/releases — tokio 1.50.0 release notes
22. https://docs.rs/crate/tokio/latest/source/CHANGELOG.md — tokio 1.x changelog
23. https://www.warp.dev/blog/adventures-text-rendering-kerning-glyph-atlases — Warp glyph atlas design
24. https://github.com/raphamorim/rio — Rio terminal (wgpu-based reference implementation)
25. https://deepwiki.com/microsoft/terminal/3.2-atlas-engine — Windows Terminal Atlas Engine
26. https://contour-terminal.org/internals/text-stack/ — Contour terminal text rendering stack
27. https://github.com/gfx-rs/wgpu/issues/1551 — headless llvmpipe eglInitialize issue
28. https://github.com/actions/runner-images/issues/2998 — lavapipe on GitHub Actions discussion

---

## Uncertainty Flags

1. **cosmic-text OpenType feature control (ligature disabling):** The exact `Attrs` API for
   per-feature control (disabling `liga`, `calt`) in cosmic-text 0.18 was not confirmed from
   documentation alone. The 0.17.0 release removed `Attrs::matches`, and the replacement API
   for feature-level control needs to be verified against the 0.18 source code before
   implementation. The safe Phase 1 fallback is to use a monospace font family that ships
   without ligatures by default.

2. **glyphon 0.9 exact Rust MSRV:** Confirmed at Rust 1.92 from the GitHub README. Verify
   this has not changed in a patch release, as it may affect CI toolchain configuration.

3. **wgpu 28 Metal-specific surface gotchas:** The general macOS requirement (create surface
   in `resumed()`) is well-documented, but wgpu 28 may have introduced additional Metal-
   specific constraints not yet covered in tutorial documentation. Run a minimal wgpu+winit
   smoke test on macOS Apple Silicon early in Phase 1 to surface any issues.

4. **portable-pty 0.9 on Windows ConPTY:** The crate claims transparent Windows support, but
   ConPTY behavior on Windows Server 2019 (the GitHub Actions `windows-latest` runner) may
   differ from Windows 11. PTY resize events in particular have known inconsistencies on older
   ConPTY versions. Verify CI behavior on Windows before relying on PTY resize in tests.

5. **tokio main thread + winit on Windows:** The pattern of running tokio separately from the
   winit event loop has been confirmed for macOS. Windows has different main-thread requirements
   that should be verified; the `rt.enter()` approach is believed to be safe but was not
   confirmed with a Windows-specific source during this research pass.
