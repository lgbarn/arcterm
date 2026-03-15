//! arcterm-render — wgpu renderer, glyph atlas, and text shaping.

pub mod gpu;
pub mod palette;
pub mod quad;
pub mod renderer;
pub mod structured;
pub mod text;

pub use gpu::GpuState;
pub use palette::RenderPalette;
pub use quad::{QuadInstance, QuadRenderer};
pub use renderer::{
    OverlayQuad, PaneRenderInfo, Renderer, build_quad_instances_at, render_tab_bar_quads,
    tab_bar_height,
};
pub use text::{ClipRect, TextRenderer};
