use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle};
use std::sync::Arc;
use wgpu::*;

use crate::geom::Rect;

/// Selection rect uniform — in [0,1] fraction of the surface.
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct SelectionUniform {
    /// x, y = top-left in [0,1]; z, w = size in [0,1].
    /// z <= 0 or w <= 0 means no selection.
    pub rect: [f32; 4],
}

impl SelectionUniform {
    pub fn none() -> Self {
        Self { rect: [0.0, 0.0, 0.0, 0.0] }
    }

    /// Create from a Rect in pixel coordinates, relative to the output's local origin.
    /// `surface_size` is (width, height) of the output surface.
    pub fn from_rect(rect: &Rect, surface_size: (u32, u32)) -> Self {
        if rect.is_empty() {
            return Self::none();
        }
        let sw = surface_size.0 as f32;
        let sh = surface_size.1 as f32;
        if sw <= 0.0 || sh <= 0.0 {
            return Self::none();
        }
        Self {
            rect: [
                rect.x as f32 / sw,
                rect.y as f32 / sh,
                rect.w as f32 / sw,
                rect.h as f32 / sh,
            ],
        }
    }
}

/// Vertex for handles/border (position in clip-space + color).
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct ColoredVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

impl ColoredVertex {
    const DESC: VertexBufferLayout<'static> = VertexBufferLayout {
        array_stride: std::mem::size_of::<Self>() as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &[
            VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: VertexFormat::Float32x2,
            },
            VertexAttribute {
                offset: std::mem::size_of::<[f32; 2]>() as u64,
                shader_location: 1,
                format: VertexFormat::Float32x4,
            },
        ],
    };
}

/// Vertex for mosaic quads (position in clip-space + UV).
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct TexturedVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
}

impl TexturedVertex {
    pub const DESC: VertexBufferLayout<'static> = VertexBufferLayout {
        array_stride: std::mem::size_of::<Self>() as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &[
            VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: VertexFormat::Float32x2,
            },
            VertexAttribute {
                offset: std::mem::size_of::<[f32; 2]>() as u64,
                shader_location: 1,
                format: VertexFormat::Float32x2,
            },
        ],
    };
}

/// Uploaded texture with both a bind group and the texture view.
pub struct UploadedTexture {
    pub bind_group: BindGroup,
    pub view: TextureView,
}

/// Shared GPU resources.
pub struct Gpu {
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
    instance: Arc<Instance>,
    screenshot_pipeline: RenderPipeline,
    handles_pipeline: RenderPipeline,
    mosaic_pipeline: RenderPipeline,
    bind_group_layout: BindGroupLayout,
    selection_bgl: BindGroupLayout,
    sampler: Sampler,
}

impl Gpu {
    pub async fn new() -> Result<Self> {
        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::VULKAN,
            ..InstanceDescriptor::new_without_display_handle()
        });

        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .context("failed to find suitable GPU adapter")?;

        tracing::info!("using adapter: {:?}", adapter.get_info());

        let (device, queue) = adapter
            .request_device(&DeviceDescriptor {
                label: Some("takeashot gpu"),
                required_features: Features::empty(),
                required_limits: Limits::default(),
                ..Default::default()
            })
            .await
            .context("failed to request GPU device")?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);
        let instance = Arc::new(instance);

        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("screenshot bg layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let selection_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("selection uniform layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(std::num::NonZeroU64::new(std::mem::size_of::<SelectionUniform>() as u64).unwrap()),
                },
                count: None,
            }],
        });

        let screenshot_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("screenshot pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout), Some(&selection_bgl)],
            immediate_size: 0,
        });

        let handles_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("handles pipeline layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });

        let shader_screenshot = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("screenshot shader"),
            source: ShaderSource::Wgsl(include_str!("shaders/screenshot.wgsl").into()),
        });

        let shader_handles = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("handles shader"),
            source: ShaderSource::Wgsl(include_str!("shaders/handles.wgsl").into()),
        });

        let screenshot_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("screenshot pipeline"),
            layout: Some(&screenshot_layout),
            vertex: VertexState {
                module: &shader_screenshot,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(FragmentState {
                module: &shader_screenshot,
                entry_point: Some("fs"),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Bgra8UnormSrgb,
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let handles_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("handles pipeline"),
            layout: Some(&handles_layout),
            vertex: VertexState {
                module: &shader_handles,
                entry_point: Some("vs"),
                buffers: &[ColoredVertex::DESC],
                compilation_options: Default::default(),
            },
            fragment: Some(FragmentState {
                module: &shader_handles,
                entry_point: Some("fs"),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Bgra8UnormSrgb,
                    blend: Some(BlendState {
                        color: BlendComponent {
                            src_factor: BlendFactor::SrcAlpha,
                            dst_factor: BlendFactor::OneMinusSrcAlpha,
                            operation: BlendOperation::Add,
                        },
                        alpha: BlendComponent {
                            src_factor: BlendFactor::SrcAlpha,
                            dst_factor: BlendFactor::OneMinusSrcAlpha,
                            operation: BlendOperation::Add,
                        },
                    }),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                ..PrimitiveState::default()
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let mosaic_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("mosaic pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let shader_mosaic = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("mosaic shader"),
            source: ShaderSource::Wgsl(include_str!("shaders/mosaic.wgsl").into()),
        });

        let mosaic_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("mosaic pipeline"),
            layout: Some(&mosaic_layout),
            vertex: VertexState {
                module: &shader_mosaic,
                entry_point: Some("vs"),
                buffers: &[TexturedVertex::DESC],
                compilation_options: Default::default(),
            },
            fragment: Some(FragmentState {
                module: &shader_mosaic,
                entry_point: Some("fs"),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Bgra8UnormSrgb,
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                ..PrimitiveState::default()
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("takeashot sampler"),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            ..Default::default()
        });

        Ok(Self {
            device,
            queue,
            instance,
            screenshot_pipeline,
            handles_pipeline,
            mosaic_pipeline,
            bind_group_layout,
            selection_bgl,
            sampler,
        })
    }

    /// Create a wgpu Surface from raw Wayland display/surface pointers.
    pub fn create_surface_from_wayland(
        &self,
        display: *mut std::ffi::c_void,
        surface: *mut std::ffi::c_void,
    ) -> Result<Surface<'static>> {
        let display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
            std::ptr::NonNull::new(display).context("null wl_display")?,
        ));
        let window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(
            std::ptr::NonNull::new(surface).context("null wl_surface")?,
        ));

        let target = SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(display_handle),
            raw_window_handle: window_handle,
        };

        // SAFETY: caller guarantees display and surface pointers are valid
        // for the lifetime of the Wayland connection/surface.
        let surface = unsafe { self.instance.create_surface_unsafe(target)? };
        Ok(surface)
    }

    /// Upload BGRA pixel data as a texture, returns the bind group and texture view.
    pub fn upload_bgra_texture(
        &self,
        width: u32,
        height: u32,
        stride: u32,
        bgra: &[u8],
    ) -> UploadedTexture {
        let texture = self.device.create_texture(&TextureDescriptor {
            label: Some("screenshot texture"),
            size: Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Bgra8UnormSrgb,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let row_bytes = width * 4;
        for y in 0..height {
            let src_offset = y as usize * stride as usize;
            let src_row = &bgra[src_offset..src_offset + row_bytes as usize];
            self.queue.write_texture(
                TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: Origin3d { x: 0, y, z: 0 },
                    aspect: TextureAspect::All,
                },
                src_row,
                TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row_bytes), rows_per_image: Some(1) },
                Extent3d { width, height: 1, depth_or_array_layers: 1 },
            );
        }

        let view = texture.create_view(&TextureViewDescriptor::default());

        let bind_group = self.device.create_bind_group(&BindGroupDescriptor {
            label: Some("screenshot bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                BindGroupEntry { binding: 0, resource: BindingResource::TextureView(&view) },
                BindGroupEntry { binding: 1, resource: BindingResource::Sampler(&self.sampler) },
            ],
        });

        UploadedTexture { bind_group, view }
    }

    /// Create a bind group for the selection uniform buffer.
    pub fn create_selection_bind_group(&self, buffer: &Buffer) -> BindGroup {
        self.device.create_bind_group(&BindGroupDescriptor {
            label: Some("selection bind group"),
            layout: &self.selection_bgl,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    }

    /// Create a uniform buffer for the selection rect.
    pub fn create_selection_buffer(&self) -> Buffer {
        self.device.create_buffer(&BufferDescriptor {
            label: Some("selection uniform buffer"),
            size: std::mem::size_of::<SelectionUniform>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Maximum vertices for selection geometry:
    /// 4 border quads × 6 + 8 handle quads × 6 = 72
    const MAX_SELECTION_VERTICES: u64 = 72;

    /// Maximum vertices for annotation geometry (dynamic, pre-allocated).
    pub const MAX_ANNOTATION_VERTICES: u64 = 65536;

    /// Create a pre-allocated vertex buffer for selection geometry.
    pub fn create_selection_vertex_buffer(&self) -> Buffer {
        self.device.create_buffer(&BufferDescriptor {
            label: Some("selection vertex buffer"),
            size: Self::MAX_SELECTION_VERTICES * std::mem::size_of::<ColoredVertex>() as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Create a pre-allocated vertex buffer for annotation geometry.
    pub fn create_annotation_vertex_buffer(&self) -> Buffer {
        self.device.create_buffer(&BufferDescriptor {
            label: Some("annotation vertex buffer"),
            size: Self::MAX_ANNOTATION_VERTICES * std::mem::size_of::<ColoredVertex>() as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Maximum vertices for mosaic quad geometry.
    pub const MAX_MOSAIC_VERTICES: u64 = 1024;

    /// Create a pre-allocated vertex buffer for mosaic quad geometry.
    pub fn create_mosaic_vertex_buffer(&self) -> Buffer {
        self.device.create_buffer(&BufferDescriptor {
            label: Some("mosaic vertex buffer"),
            size: Self::MAX_MOSAIC_VERTICES * std::mem::size_of::<TexturedVertex>() as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Build the vertices for selection border + handles.
    /// Returns vertex data as a Vec (caller writes to pre-allocated buffer).
    /// When `include_handles` is false, only the border is drawn (for Pending/Creating states).
    pub fn build_selection_vertices(
        rect: &Rect,
        surface_size: (u32, u32),
        include_handles: bool,
    ) -> Vec<ColoredVertex> {
        let sw = surface_size.0 as f32;
        let sh = surface_size.1 as f32;
        if sw <= 0.0 || sh <= 0.0 || rect.is_empty() {
            return Vec::new();
        }

        // Convert pixel rect to NDC (clip space)
        let l = rect.x as f32 / sw * 2.0 - 1.0;
        let r = (rect.x + rect.w) as f32 / sw * 2.0 - 1.0;
        let t = 1.0 - rect.y as f32 / sh * 2.0;
        let b = 1.0 - (rect.y + rect.h) as f32 / sh * 2.0;

        let border_color: [f32; 4] = [0.27, 0.53, 0.87, 1.0]; // KDE blue
        let handle_color: [f32; 4] = [1.0, 1.0, 1.0, 1.0]; // white

        let mut vertices: Vec<ColoredVertex> = Vec::with_capacity(72);

        // Border: 4 rectangles (top, bottom, left, right), 2px wide
        let bw = 2.0 / sw * 2.0; // border width in NDC
        let bh = 2.0 / sh * 2.0;

        // Top border
        vertices.extend_from_slice(&quad(l - bw, t, r + bw, t + bh, border_color));
        // Bottom border
        vertices.extend_from_slice(&quad(l - bw, b - bh, r + bw, b, border_color));
        // Left border
        vertices.extend_from_slice(&quad(l - bw, t + bh, l, b - bh, border_color));
        // Right border
        vertices.extend_from_slice(&quad(r, t + bh, r + bw, b - bh, border_color));

        // 8 handles: 6x6 pixel squares (only for Confirmed state)
        if include_handles {
            let hs = 3.0; // half-size in pixels
            let hsx = hs / sw * 2.0;
            let hsy = hs / sh * 2.0;

            let mx = (l + r) / 2.0;
            let my = (t + b) / 2.0;

            let handles = [
                (l, t), (mx, t), (r, t),
                (l, my), (r, my),
                (l, b), (mx, b), (r, b),
            ];

            for (hx, hy) in &handles {
                vertices.extend_from_slice(&quad(
                    hx - hsx, hy - hsy, hx + hsx, hy + hsy,
                    handle_color,
                ));
            }
        }

        vertices
    }

    /// Acquire a surface texture and return it, without presenting.
    /// Returns `None` if the surface is not ready (timeout, occluded, outdated).
    pub fn acquire_surface_texture(
        &self,
        surface: &Surface,
        config: &SurfaceConfiguration,
    ) -> Result<Option<SurfaceTexture>> {
        match surface.get_current_texture() {
            CurrentSurfaceTexture::Success(t) | CurrentSurfaceTexture::Suboptimal(t) => Ok(Some(t)),
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => Ok(None),
            CurrentSurfaceTexture::Outdated => {
                surface.configure(&self.device, config);
                Ok(None)
            }
            CurrentSurfaceTexture::Lost | CurrentSurfaceTexture::Validation => {
                anyhow::bail!("surface lost or validation error");
            }
        }
    }

    /// Render passes into an existing texture view (does NOT acquire or present the surface).
    /// Caller is responsible for acquiring the surface texture and presenting it.
    pub fn render_into(
        &self,
        view: &TextureView,
        bg_bind_group: &BindGroup,
        selection_bind_group: &BindGroup,
        selection_vertex_buffer: Option<(&Buffer, u32)>,
        annotation_vertex_buffer: Option<(&Buffer, u32)>,
        mosaic: Option<(&BindGroup, &Buffer, u32)>,
    ) {
        let mut encoder = self.device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("overlay render"),
        });

        // Pass 1: draw screenshot with selection-aware dimming
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("screenshot pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations { load: LoadOp::Clear(Color::BLACK), store: StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.screenshot_pipeline);
            pass.set_bind_group(0, bg_bind_group, &[]);
            pass.set_bind_group(1, selection_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        // Pass 2: draw mosaic quads (blurred regions over the screenshot)
        if let Some((blurred_bg, vbuf, count)) = mosaic {
            if count > 0 {
                let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("mosaic pass"),
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: Operations { load: LoadOp::Load, store: StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.mosaic_pipeline);
                pass.set_bind_group(0, blurred_bg, &[]);
                pass.set_vertex_buffer(0, vbuf.slice(..));
                pass.draw(0..count, 0..1);
            }
        }

        // Pass 3: draw annotations
        if let Some((vbuf, count)) = annotation_vertex_buffer {
            if count > 0 {
                let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("annotation pass"),
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: Operations { load: LoadOp::Load, store: StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.handles_pipeline);
                pass.set_vertex_buffer(0, vbuf.slice(..));
                pass.draw(0..count, 0..1);
            }
        }

        // Pass 4: draw selection handles and border
        if let Some((vbuf, count)) = selection_vertex_buffer {
            if count > 0 {
                let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("handles pass"),
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: Operations { load: LoadOp::Load, store: StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.handles_pipeline);
                pass.set_vertex_buffer(0, vbuf.slice(..));
                pass.draw(0..count, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }
}

/// Build 6 vertices (2 triangles) for a filled rectangle.
fn quad(l: f32, t: f32, r: f32, b: f32, color: [f32; 4]) -> [ColoredVertex; 6] {
    [
        ColoredVertex { position: [l, t], color },
        ColoredVertex { position: [r, t], color },
        ColoredVertex { position: [l, b], color },
        ColoredVertex { position: [l, b], color },
        ColoredVertex { position: [r, t], color },
        ColoredVertex { position: [r, b], color },
    ]
}
