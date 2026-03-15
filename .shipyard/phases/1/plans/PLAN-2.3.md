---
phase: foundation
plan: "2.3"
wave: 2
dependencies: ["1.1"]
must_haves:
  - wgpu + winit window opens on macOS
  - glyphon renders monospace text in the window
  - Window responds to resize events
  - Surface creation happens in resumed() per macOS requirements
files_touched:
  - arcterm-render/src/lib.rs
  - arcterm-render/src/gpu.rs
  - arcterm-render/src/text.rs
  - arcterm-render/src/renderer.rs
tdd: false
---

# Plan 2.3 -- GPU Window and Text Rendering

**Wave 2** | Depends on: Plan 1.1 (arcterm-core types) | Parallel with: Plans 2.1, 2.2

## Goal

Create a wgpu-backed window using winit 0.30's ApplicationHandler pattern, initialize glyphon for text rendering, and render a static grid of monospace text. After this plan, running the render crate's example opens a window displaying colored monospace text.

---

<task id="1" files="arcterm-render/src/gpu.rs, arcterm-render/src/lib.rs" tdd="false">
  <action>
    Implement `GpuState` in `arcterm-render/src/gpu.rs` that encapsulates all wgpu initialization.

    **Structure:**
    ```rust
    pub struct GpuState {
        pub device: wgpu::Device,
        pub queue: wgpu::Queue,
        pub surface: wgpu::Surface<'static>,
        pub surface_config: wgpu::SurfaceConfiguration,
        pub surface_format: wgpu::TextureFormat,
    }
    ```

    **`GpuState::new(window: Arc<Window>) -> Self`:**
    Use `pollster::block_on` to run the async init inside synchronous `resumed()`. Follow the exact initialization sequence from RESEARCH.md:
    1. Create `wgpu::Instance` with `Backends::all()`.
    2. Create surface from `window.clone()`.
    3. Request adapter with `PowerPreference::HighPerformance`, `compatible_surface: Some(&surface)`.
    4. Request device with default features and downlevel_webgl2_defaults limits (using adapter resolution).
    5. Configure surface: get capabilities, select first format (prefer sRGB), `PresentMode::Fifo`, dimensions from `window.inner_size()`.

    **`GpuState::resize(&mut self, width: u32, height: u32)`:**
    Reconfigure surface with new dimensions. Guard against width=0 or height=0 (minimized window).

    **`GpuState::begin_frame(&self) -> Result<(wgpu::SurfaceTexture, wgpu::TextureView), wgpu::SurfaceError>`:**
    Get current texture, create view. Caller handles `SurfaceError::Lost` by calling resize.

    **`lib.rs`:** Re-export `GpuState`.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo check --package arcterm-render 2>&1 | tail -5</verify>
  <done>`cargo check --package arcterm-render` passes. `GpuState` struct compiles with correct wgpu 28 API usage. No GPU runtime test at this stage (tested visually in task 3).</done>
</task>

<task id="2" files="arcterm-render/src/text.rs" tdd="false">
  <action>
    Implement `TextRenderer` in `arcterm-render/src/text.rs` that wraps glyphon for terminal text rendering.

    **Structure:**
    ```rust
    pub struct TextRenderer {
        font_system: glyphon::FontSystem,
        swash_cache: glyphon::SwashCache,
        atlas: glyphon::TextAtlas,
        text_renderer: glyphon::TextRenderer,
        viewport: glyphon::Viewport,
        cache: glyphon::Cache,
        cell_width: f32,
        cell_height: f32,
    }
    ```

    **`TextRenderer::new(device: &wgpu::Device, queue: &wgpu::Queue, surface_format: wgpu::TextureFormat) -> Self`:**
    1. Create `glyphon::Cache`, `glyphon::Viewport`, `glyphon::TextAtlas`, `glyphon::TextRenderer`.
    2. Create `glyphon::FontSystem::new()` (auto-detects system fonts).
    3. Create `glyphon::SwashCache::new()`.
    4. Measure cell dimensions: create a temporary `glyphon::Buffer`, set text to "M" with monospace family, shape it, extract glyph advance for `cell_width`. Use `line_height` from metrics for `cell_height`. Use font size 16.0 and line height 20.0 as defaults.

    **`TextRenderer::prepare_grid(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, grid: &Grid, surface_width: u32, surface_height: u32, scale_factor: f64)`:**
    1. Update viewport resolution.
    2. Build a `glyphon::Buffer` from the grid content: for each row, construct a line of text from the cell characters. Use `glyphon::Attrs` to set foreground color per-cell span. The key insight: construct the buffer as a single text block with newlines between rows, using per-character color attributes via `attrs_list`.
    3. Alternative simpler approach for Phase 1: create one `glyphon::Buffer` per row. Each row buffer is positioned at `(0.0, row_index * cell_height * scale_factor)`. This avoids complex per-character attrs spanning and is easier to reason about.
    4. Map `arcterm_core::Color` to `glyphon::Color`:
       - `Color::Default` -> white (fg) or black (bg) -- hardcoded Phase 1 palette.
       - `Color::Indexed(n)` -> standard ANSI 16-color palette lookup, then extended 256-color palette.
       - `Color::Rgb(r, g, b)` -> `glyphon::Color::rgb(r, g, b)`.
    5. Call `self.text_renderer.prepare(...)` with all row TextAreas.

    **`TextRenderer::render(&self, atlas: &glyphon::TextAtlas, viewport: &glyphon::Viewport, render_pass: &mut wgpu::RenderPass)`:**
    Delegate to `self.text_renderer.render(...)`. Then call `self.atlas.trim()`.

    **`TextRenderer::cell_size(&self) -> (f32, f32)`:** Return `(cell_width, cell_height)`.

    **Color palette:** Include a `const ANSI_COLORS: [glyphon::Color; 16]` array with the standard terminal color palette (black, red, green, yellow, blue, magenta, cyan, white, plus bright variants). For 256-color, compute the 6x6x6 color cube and greyscale ramp programmatically.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo check --package arcterm-render 2>&1 | tail -5</verify>
  <done>`cargo check --package arcterm-render` passes. TextRenderer compiles with correct glyphon 0.9 API. Color palette covers all 256 indexed colors.</done>
</task>

<task id="3" files="arcterm-render/src/renderer.rs, arcterm-render/src/lib.rs" tdd="false">
  <action>
    Implement `Renderer` in `arcterm-render/src/renderer.rs` that combines `GpuState` and `TextRenderer` into a single rendering interface, and create a runnable example binary that opens a window and renders a test grid.

    **Structure:**
    ```rust
    pub struct Renderer {
        gpu: GpuState,
        text: TextRenderer,
    }
    ```

    **`Renderer::new(window: Arc<Window>) -> Self`:**
    Create GpuState, then TextRenderer using the device, queue, and format from GpuState.

    **`Renderer::resize(&mut self, width: u32, height: u32)`:**
    Delegate to `gpu.resize()`.

    **`Renderer::render_frame(&mut self, grid: &Grid, scale_factor: f64) -> Result<(), wgpu::SurfaceError>`:**
    1. Call `text.prepare_grid(...)` with grid and surface dimensions.
    2. Call `gpu.begin_frame()` to get surface texture and view.
    3. Create a `wgpu::CommandEncoder`.
    4. Begin a render pass with the view as color attachment, clear to dark background color (e.g., RGB 30, 30, 46 -- Catppuccin-style dark).
    5. Call `text.render(...)` within the render pass.
    6. Drop render pass, submit command buffer to queue.
    7. Present surface texture.
    8. Return Ok(()).

    **`Renderer::grid_size_for_window(&self, width: u32, height: u32, scale_factor: f64) -> GridSize`:**
    Calculate how many rows/cols fit in the window based on cell dimensions and scale factor.

    **`lib.rs`:** Re-export `Renderer`, `GpuState`, `TextRenderer`.

    **Example binary (arcterm-render/examples/window.rs):**
    Create a minimal winit ApplicationHandler that:
    1. In `resumed()`: create window (800x600, title "arcterm-render test"), create Renderer.
    2. Create a test Grid (80x24) and populate it with: row 0 = "Hello, Arcterm!" in default color, row 1 = "Red text" with fg=Indexed(1), row 2 = "Green text" with fg=Indexed(2), row 3 = "Bold blue" with fg=Indexed(4) and bold=true.
    3. In `window_event(RedrawRequested)`: call `renderer.render_frame(&grid)`.
    4. In `window_event(Resized)`: call `renderer.resize()`.
    5. In `window_event(CloseRequested)`: exit.
    6. In `about_to_wait`: call `window.request_redraw()`.

    This example is the visual smoke test for the entire rendering pipeline.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo build --package arcterm-render --example window 2>&1 | tail -5</verify>
  <done>`cargo build --package arcterm-render --example window` succeeds. Running `cargo run --package arcterm-render --example window` opens a window on macOS displaying colored monospace text on a dark background. Window resizes without crashing. Closing the window exits cleanly.</done>
</task>
