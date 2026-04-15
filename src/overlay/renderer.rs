use anyhow::{Context, Result};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle};
use std::sync::Arc;
use wgpu::*;

/// Shared GPU resources.
pub struct Gpu {
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
    instance: Arc<Instance>,
    screenshot_pipeline: RenderPipeline,
    overlay_pipeline: RenderPipeline,
    bind_group_layout: BindGroupLayout,
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

        let screenshot_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("screenshot pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let overlay_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("overlay pipeline layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });

        let shader_screenshot = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("screenshot shader"),
            source: ShaderSource::Wgsl(include_str!("shaders/screenshot.wgsl").into()),
        });

        let shader_overlay = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("overlay shader"),
            source: ShaderSource::Wgsl(include_str!("shaders/overlay.wgsl").into()),
        });

        let _common_vertex = VertexState {
            module: &shader_screenshot, // Just for the vertex shader, same for both
            entry_point: Some("vs"),
            buffers: &[],
            compilation_options: Default::default(),
        };

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

        let overlay_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("overlay pipeline"),
            layout: Some(&overlay_layout),
            vertex: VertexState {
                module: &shader_overlay,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(FragmentState {
                module: &shader_overlay,
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
            primitive: PrimitiveState::default(),
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
            overlay_pipeline,
            bind_group_layout,
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

    /// Upload BGRA pixel data as a texture, returns the bind group for rendering.
    pub fn upload_bgra_texture(
        &self,
        width: u32,
        height: u32,
        stride: u32,
        bgra: &[u8],
    ) -> BindGroup {
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

        self.device.create_bind_group(&BindGroupDescriptor {
            label: Some("screenshot bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                BindGroupEntry { binding: 0, resource: BindingResource::TextureView(&view) },
                BindGroupEntry { binding: 1, resource: BindingResource::Sampler(&self.sampler) },
            ],
        })
    }

    /// Render a single frame: screenshot background + dark overlay.
    pub fn render(
        &self,
        surface: &Surface,
        config: &SurfaceConfiguration,
        bg_bind_group: &BindGroup,
    ) -> Result<()> {
        let output = match surface.get_current_texture() {
            CurrentSurfaceTexture::Success(t) | CurrentSurfaceTexture::Suboptimal(t) => t,
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => {
                // Skip this frame, try again later.
                return Ok(());
            }
            CurrentSurfaceTexture::Outdated => {
                // Reconfigure and skip.
                surface.configure(&self.device, config);
                return Ok(());
            }
            CurrentSurfaceTexture::Lost | CurrentSurfaceTexture::Validation => {
                anyhow::bail!("surface lost or validation error");
            }
        };

        let view = output.texture.create_view(&TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("overlay render"),
        });

        // Pass 1: draw screenshot
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("screenshot pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
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
            pass.draw(0..3, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}