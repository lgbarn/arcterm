//! arcterm-render — wgpu renderer, glyph atlas, and text shaping.

pub mod gpu;
pub mod palette;
pub mod quad;
pub mod renderer;
pub mod text;

pub use gpu::GpuState;
pub use palette::RenderPalette;
pub use quad::{QuadInstance, QuadRenderer};
pub use renderer::Renderer;
pub use text::TextRenderer;
