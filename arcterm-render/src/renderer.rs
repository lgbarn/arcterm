//! High-level Renderer: combines GpuState + TextRenderer.

use std::sync::Arc;

use arcterm_core::{Grid, GridSize};
use winit::window::Window;

use crate::gpu::GpuState;
use crate::text::TextRenderer;

/// Default font size (logical pixels / points).
const FONT_SIZE: f32 = 14.0;

/// Dark background color (30, 30, 46 in 0–1 range).
const BG_COLOR: wgpu::Color = wgpu::Color {
    r: 30.0 / 255.0,
    g: 30.0 / 255.0,
    b: 46.0 / 255.0,
    a: 1.0,
};

/// Top-level renderer: owns GPU state and the text renderer.
pub struct Renderer {
    pub gpu: GpuState,
    pub text: TextRenderer,
}

impl Renderer {
    /// Create the renderer, initializing wgpu and glyphon.
    pub fn new(window: Arc<Window>) -> Self {
        let gpu = GpuState::new(window);
        let text = TextRenderer::new(
            &gpu.device,
            &gpu.queue,
            gpu.surface_format,
            FONT_SIZE,
        );
        Self { gpu, text }
    }

    /// Handle window resize: reconfigure the surface and update dimensions.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.gpu.resize(width, height);
    }

    /// Render a full terminal grid frame.
    pub fn render_frame(&mut self, grid: &Grid, scale_factor: f64) {
        let sf = scale_factor as f32;
        let w = self.gpu.surface_config.width;
        let h = self.gpu.surface_config.height;

        self.text.update_viewport(&self.gpu.queue, w, h);

        // Prepare text — suppress errors on atlas full (rare on first frame).
        let _ = self.text.prepare_grid(&self.gpu.device, &self.gpu.queue, grid, sf);

        let (frame, view) = match self.gpu.begin_frame() {
            Ok(pair) => pair,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                // Surface was lost (common on macOS during occlusion/restoration).
                // Reconfigure with current dimensions and skip this frame.
                self.gpu.resize(w, h);
                return;
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                log::error!("GPU out of memory — skipping frame");
                return;
            }
            Err(e) => {
                log::warn!("begin_frame error: {e:?} — skipping frame");
                return;
            }
        };

        let mut encoder =
            self.gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("arcterm frame encoder"),
                });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("arcterm render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(BG_COLOR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            let _ = self.text.render(&mut pass);
        }

        self.gpu.queue.submit(Some(encoder.finish()));
        frame.present();
    }

    /// Calculate how many columns × rows fit the window at the given scale.
    pub fn grid_size_for_window(&self, width: u32, height: u32, scale_factor: f64) -> GridSize {
        let sf = scale_factor as f32;
        let cell_w = self.text.cell_size.width * sf;
        let cell_h = self.text.cell_size.height * sf;
        if cell_w <= 0.0 || cell_h <= 0.0 {
            return GridSize::new(24, 80);
        }
        let cols = ((width as f32) / cell_w).floor() as usize;
        let rows = ((height as f32) / cell_h).floor() as usize;
        GridSize::new(rows.max(1), cols.max(1))
    }
}
