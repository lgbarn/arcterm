//! arcterm-render — wgpu renderer, glyph atlas, and text shaping.

pub mod gpu;
pub mod text;

pub use gpu::GpuState;
pub use text::TextRenderer;
