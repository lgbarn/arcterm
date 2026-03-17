//! Textured quad renderer for Kitty Graphics Protocol inline images.
//!
//! This is a **separate** wgpu pipeline from [`crate::quad::QuadRenderer`]
//! (which renders solid-color quads).  This pipeline uses a WGSL vertex +
//! fragment shader that interpolates UV coordinates and samples from a
//! `texture_2d<f32>` via a linear sampler.
//!
//! # Texture alignment
//!
//! `wgpu` requires `bytes_per_row` to be a multiple of
//! `wgpu::COPY_BYTES_PER_ROW_ALIGNMENT` (256).  [`ImageQuadRenderer::create_texture`]
//! handles the padding automatically.

use bytemuck::{Pod, Zeroable};

// ---------------------------------------------------------------------------
// Vertex type
// ---------------------------------------------------------------------------

/// One vertex of a textured quad.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ImageVertex {
    /// Position in pixel coordinates (origin = top-left of screen).
    pub position: [f32; 2],
    /// UV texture coordinates in [0, 1].
    pub uv: [f32; 2],
}

impl ImageVertex {
    const ATTRS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
        0 => Float32x2, // position
        1 => Float32x2, // uv
    ];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ImageVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRS,
        }
    }
}

// ---------------------------------------------------------------------------
// Screen uniform (viewport dimensions for pixel→clip-space conversion)
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct ScreenUniform {
    resolution: [f32; 2],
    _pad: [f32; 2],
}

// ---------------------------------------------------------------------------
// ImageTexture — one uploaded GPU texture with its bind group
// ---------------------------------------------------------------------------

/// A decoded image uploaded to the GPU as a `wgpu::Texture`.
///
/// The `bind_group` binds the texture view + sampler for the image pipeline.
/// Drop this value to release GPU memory.
pub struct ImageTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub bind_group: wgpu::BindGroup,
    pub width: u32,
    pub height: u32,
}

// ---------------------------------------------------------------------------
// ImageQuadRenderer
// ---------------------------------------------------------------------------

/// GPU pipeline for rendering RGBA images as textured quads.
///
/// Call [`ImageQuadRenderer::new`] once at startup, then per-frame:
/// 1. [`create_texture`](Self::create_texture) for each new image.
/// 2. [`prepare`](Self::prepare) before the render pass to upload vertices.
/// 3. [`render`](Self::render) inside the active `RenderPass`.
pub struct ImageQuadRenderer {
    pipeline: wgpu::RenderPipeline,
    /// Bind group layout shared by all image textures (texture + sampler).
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// Per-quad vertex buffer — all quads for the frame packed contiguously.
    vertex_buffer: wgpu::Buffer,
    /// Viewport uniform buffer.
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    /// Number of images prepared for the current frame (set by `prepare`).
    prepared_count: usize,
}

impl ImageQuadRenderer {
    /// Maximum number of images that can be rendered in a single frame.
    const MAX_IMAGES: usize = 256;
    /// Vertices per quad (two triangles).
    const VERTS_PER_QUAD: usize = 6;

    /// Create the image quad pipeline.  Call once at GPU initialization.
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        // ---- Shader --------------------------------------------------------
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("image quad shader"),
            source: wgpu::ShaderSource::Wgsl(IMAGE_QUAD_WGSL.into()),
        });

        // ---- Viewport uniform ----------------------------------------------
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image quad uniform buffer"),
            size: std::mem::size_of::<ScreenUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("image quad uniform bgl"),
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

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("image quad uniform bg"),
            layout: &uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // ---- Texture bind group layout (texture + sampler) -----------------
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("image quad texture bgl"),
            entries: &[
                // binding 0: texture
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // binding 1: sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // ---- Sampler -------------------------------------------------------
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("image quad sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        // ---- Pipeline layout -----------------------------------------------
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("image quad pipeline layout"),
            bind_group_layouts: &[&uniform_bgl, &bind_group_layout],
            ..Default::default()
        });

        // ---- Render pipeline -----------------------------------------------
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("image quad render pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[ImageVertex::desc()],
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
            label: Some("image quad vertex buffer"),
            size: (Self::MAX_IMAGES * Self::VERTS_PER_QUAD * std::mem::size_of::<ImageVertex>())
                as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            vertex_buffer,
            uniform_buffer,
            uniform_bind_group,
            prepared_count: 0,
        }
    }

    /// Upload RGBA pixel data to the GPU and return an [`ImageTexture`].
    ///
    /// `rgba_bytes` must be exactly `width * height * 4` bytes.
    ///
    /// Handles wgpu's 256-byte `bytes_per_row` alignment requirement by padding
    /// each row with zeroed bytes when necessary.
    pub fn create_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rgba_bytes: &[u8],
        width: u32,
        height: u32,
    ) -> ImageTexture {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("kitty image texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // Compute aligned bytes_per_row: must be a multiple of 256.
        let unaligned_bpr = width * 4;
        let aligned_bpr = (unaligned_bpr + 255) & !255;

        // Build padded data if alignment requires it.
        let upload_data: std::borrow::Cow<[u8]> = if aligned_bpr == unaligned_bpr {
            std::borrow::Cow::Borrowed(rgba_bytes)
        } else {
            let mut padded = vec![0u8; (aligned_bpr * height) as usize];
            for row in 0..height as usize {
                let src_start = row * unaligned_bpr as usize;
                let src_end = src_start + unaligned_bpr as usize;
                let dst_start = row * aligned_bpr as usize;
                let dst_end = dst_start + unaligned_bpr as usize;
                padded[dst_start..dst_end].copy_from_slice(&rgba_bytes[src_start..src_end]);
            }
            std::borrow::Cow::Owned(padded)
        };

        queue.write_texture(
            texture.as_image_copy(),
            &upload_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(aligned_bpr),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("kitty image bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        ImageTexture {
            texture,
            view,
            bind_group,
            width,
            height,
        }
    }

    /// Prepare image quads for rendering.
    ///
    /// Call this **before** beginning the render pass.  Uploads vertex data and
    /// the viewport uniform to the GPU.  Returns the number of images prepared.
    ///
    /// `placements` is a slice of `(image_texture_ref, rect)` pairs where `rect`
    /// is `[x, y, width, height]` in **physical pixels** (top-left origin, y-down).
    pub fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        placements: &[(&ImageTexture, [f32; 4])],
        viewport_width: u32,
        viewport_height: u32,
    ) -> usize {
        let count = placements.len().min(Self::MAX_IMAGES);
        if count == 0 {
            self.prepared_count = 0;
            return 0;
        }

        // Upload viewport uniform.
        let uniform = ScreenUniform {
            resolution: [viewport_width as f32, viewport_height as f32],
            _pad: [0.0; 2],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));

        // Build all vertices for the frame into one contiguous buffer.
        let mut vertices: Vec<ImageVertex> = Vec::with_capacity(count * Self::VERTS_PER_QUAD);

        for (_, rect) in placements.iter().take(count) {
            let [x, y, w, h] = *rect;
            let tl = ImageVertex {
                position: [x, y],
                uv: [0.0, 0.0],
            };
            let tr = ImageVertex {
                position: [x + w, y],
                uv: [1.0, 0.0],
            };
            let bl = ImageVertex {
                position: [x, y + h],
                uv: [0.0, 1.0],
            };
            let br = ImageVertex {
                position: [x + w, y + h],
                uv: [1.0, 1.0],
            };
            vertices.extend_from_slice(&[tl, tr, bl, tr, br, bl]);
        }

        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        self.prepared_count = count;
        count
    }

    /// Record image quad draw calls into the active render pass.
    ///
    /// Must be called after [`prepare`](Self::prepare).
    ///
    /// `placements` must be the same slice that was passed to `prepare` so
    /// the vertex offsets and bind groups align.
    pub fn render<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        placements: &'pass [(&'pass ImageTexture, [f32; 4])],
    ) {
        let count = self.prepared_count.min(placements.len());
        if count == 0 {
            return;
        }

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        for (i, (image, _)) in placements.iter().take(count).enumerate() {
            let vertex_start = (i * Self::VERTS_PER_QUAD) as u32;
            let vertex_end = vertex_start + Self::VERTS_PER_QUAD as u32;
            pass.set_bind_group(1, &image.bind_group, &[]);
            pass.draw(vertex_start..vertex_end, 0..1);
        }
    }
}

// ---------------------------------------------------------------------------
// WGSL shader — textured quad
// ---------------------------------------------------------------------------

const IMAGE_QUAD_WGSL: &str = r#"
struct ScreenUniform {
    resolution: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> screen: ScreenUniform;

@group(1) @binding(0)
var t_image: texture_2d<f32>;

@group(1) @binding(1)
var s_image: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv:       vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0)       uv:            vec2<f32>,
};

/// Transform pixel-space coordinates (origin = top-left, y-down) to
/// clip space (origin = center, y-up, range [-1, 1]).
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let clip_x =  (in.position.x / screen.resolution.x) * 2.0 - 1.0;
    let clip_y = -((in.position.y / screen.resolution.y) * 2.0 - 1.0);
    out.clip_position = vec4<f32>(clip_x, clip_y, 0.0, 1.0);
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_image, s_image, in.uv);
}
"#;
