//! Colored quad (rectangle) rendering pipeline.
//!
//! Draws solid-color rectangles in pixel space via a simple wgpu pipeline.
//! Used for cell backgrounds and the block cursor.

use bytemuck::{Pod, Zeroable};

// ---------------------------------------------------------------------------
// Vertex type
// ---------------------------------------------------------------------------

/// A single vertex of a quad triangle with its associated color.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct QuadVertex {
    /// Position in pixel coordinates (origin = top-left of screen).
    pub position: [f32; 2],
    /// RGBA color, components in [0, 1].
    pub color: [f32; 4],
}

impl QuadVertex {
    const ATTRS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x4,
    ];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<QuadVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRS,
        }
    }
}

// ---------------------------------------------------------------------------
// Instance type (caller-facing API)
// ---------------------------------------------------------------------------

/// One colored rectangle to be rendered.
#[derive(Clone, Copy, Debug)]
pub struct QuadInstance {
    /// Bounding rect in pixel coordinates: [x, y, width, height].
    pub rect: [f32; 4],
    /// RGBA color, components in [0, 1].
    pub color: [f32; 4],
}

// ---------------------------------------------------------------------------
// Uniform buffer
// ---------------------------------------------------------------------------

/// Screen resolution uniform passed to the vertex shader so it can convert
/// pixel coordinates to clip space without the CPU needing to know NDC math.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct ScreenUniform {
    /// [width, height] of the render target in pixels.
    resolution: [f32; 2],
    /// Padding to satisfy wgpu's 16-byte alignment requirement.
    _pad: [f32; 2],
}

// ---------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------

/// Maximum number of quads that can be rendered in a single frame.
const MAX_QUADS: usize = 8192;
/// Each quad is two triangles → 6 vertices.
const VERTS_PER_QUAD: usize = 6;
const MAX_VERTICES: usize = MAX_QUADS * VERTS_PER_QUAD;

/// GPU pipeline for drawing solid-color rectangles.
pub struct QuadRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Number of vertices ready to draw (set by `prepare`).
    vertex_count: u32,
}

impl QuadRenderer {
    /// Create the pipeline.  Call once at startup.
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        // ---- Shader --------------------------------------------------------
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("quad shader"),
            source: wgpu::ShaderSource::Wgsl(QUAD_WGSL.into()),
        });

        // ---- Uniform buffer ------------------------------------------------
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad uniform buffer"),
            size: std::mem::size_of::<ScreenUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ---- Bind group layout + group -------------------------------------
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("quad bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("quad bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // ---- Pipeline layout -----------------------------------------------
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("quad pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            ..Default::default()
        });

        // ---- Render pipeline -----------------------------------------------
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("quad render pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[QuadVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // ---- Vertex buffer -------------------------------------------------
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad vertex buffer"),
            size: (MAX_VERTICES * std::mem::size_of::<QuadVertex>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            vertex_buffer,
            uniform_buffer,
            bind_group,
            vertex_count: 0,
        }
    }

    /// Upload quad data to the GPU.
    ///
    /// Call once per frame before `render`.  Converts `QuadInstance` rects
    /// into two-triangle vertex fans and writes the resolution uniform.
    pub fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        quads: &[QuadInstance],
        width: u32,
        height: u32,
    ) {
        // Update resolution uniform.
        let uniform = ScreenUniform {
            resolution: [width as f32, height as f32],
            _pad: [0.0; 2],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));

        // Build vertex data.
        let mut vertices: Vec<QuadVertex> = Vec::with_capacity(quads.len() * VERTS_PER_QUAD);
        for quad in quads.iter().take(MAX_QUADS) {
            let [x, y, w, h] = quad.rect;
            let color = quad.color;
            // Two triangles (CW winding, y-down pixel space):
            //   top-left, top-right, bottom-left
            //   top-right, bottom-right, bottom-left
            let tl = QuadVertex {
                position: [x, y],
                color,
            };
            let tr = QuadVertex {
                position: [x + w, y],
                color,
            };
            let bl = QuadVertex {
                position: [x, y + h],
                color,
            };
            let br = QuadVertex {
                position: [x + w, y + h],
                color,
            };
            vertices.extend_from_slice(&[tl, tr, bl, tr, br, bl]);
        }

        self.vertex_count = vertices.len() as u32;
        if !vertices.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        }
    }

    /// Record quad draw calls into the active render pass.
    ///
    /// Must be called after `prepare` and before glyphon text rendering.
    pub fn render<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        if self.vertex_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }
}

// ---------------------------------------------------------------------------
// WGSL shader
// ---------------------------------------------------------------------------

const QUAD_WGSL: &str = r#"
struct ScreenUniform {
    resolution: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> screen: ScreenUniform;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color:    vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0)       color:         vec4<f32>,
};

/// Transform pixel-space coordinates (origin = top-left, y-down) to
/// clip space (origin = center, y-up, range [-1, 1]).
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let clip_x =  (in.position.x / screen.resolution.x) * 2.0 - 1.0;
    let clip_y = -((in.position.y / screen.resolution.y) * 2.0 - 1.0);
    out.clip_position = vec4<f32>(clip_x, clip_y, 0.0, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#;
