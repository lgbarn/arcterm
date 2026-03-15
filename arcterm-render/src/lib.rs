//! arcterm-render — wgpu renderer, glyph atlas, and text shaping.

pub mod gpu;
pub mod quad;
pub mod renderer;
pub mod text;

pub use gpu::GpuState;
pub use quad::{QuadInstance, QuadRenderer};
pub use renderer::Renderer;
pub use text::TextRenderer;
