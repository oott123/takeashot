use anyhow::Result;
use std::sync::Arc;
use wgpu::*;

const BLUR_PASSES: u32 = 3;

pub struct DualBlur {
    device: Arc<Device>,
    queue: Arc<Queue>,
    down_pipeline: RenderPipeline,
    up_pipeline: RenderPipeline,
    bgl: BindGroupLayout,
    sampler: Sampler,
}

impl DualBlur {
    pub fn new(device: Arc<Device>, queue: Arc<Queue>) -> Result<Self> {
        let bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("blur bind group layout"),
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

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("blur pipeline layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        let shader_down = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("blur downsample shader"),
            source: ShaderSource::Wgsl(include_str!("overlay/shaders/blur_down.wgsl").into()),
        });

        let shader_up = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("blur upsample shader"),
            source: ShaderSource::Wgsl(include_str!("overlay/shaders/blur_up.wgsl").into()),
        });

        let create_pipeline = |shader: &ShaderModule, label: &str| -> RenderPipeline {
            device.create_render_pipeline(&RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: VertexState {
                    module: shader,
                    entry_point: Some("vs"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(FragmentState {
                    module: shader,
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
            })
        };

        let down_pipeline = create_pipeline(&shader_down, "blur downsample pipeline");
        let up_pipeline = create_pipeline(&shader_up, "blur upsample pipeline");

        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("blur sampler"),
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
            down_pipeline,
            up_pipeline,
            bgl,
            sampler,
        })
    }

    /// Generate a blurred texture from a source texture view.
    /// `width`/`height` are the source texture dimensions.
    /// Returns the bind group for the blurred result (texture view + sampler).
    pub fn blur(&self, source_view: &TextureView, width: u32, height: u32) -> BindGroup {
        let start = std::time::Instant::now();
        tracing::info!("blur start: {width}x{height}, {BLUR_PASSES} passes");

        let mut down_textures: Vec<(Texture, TextureView, BindGroup)> = Vec::with_capacity(BLUR_PASSES as usize);
        let mut up_textures: Vec<(Texture, TextureView)> = Vec::with_capacity(BLUR_PASSES as usize);

        // Create source bind group from the provided texture view
        let source_bg = self.make_bind_group(source_view);

        // --- Downsample passes ---
        let mut prev_bg: &BindGroup = &source_bg;
        let mut cur_w = width;
        let mut cur_h = height;

        for i in 0..BLUR_PASSES {
            cur_w = (cur_w + 1) / 2;
            cur_h = (cur_h + 1) / 2;

            let (tex, view) = self.create_render_texture(cur_w, cur_h, &format!("blur down {i}"));
            let bg = self.make_bind_group(&view);

            self.run_pass(&self.down_pipeline, prev_bg, &view);

            down_textures.push((tex, view, bg));
            prev_bg = &down_textures[i as usize].2;
        }

        // --- Upsample passes ---
        // Start from the smallest downsampled level and work back up
        for i in (0..BLUR_PASSES as usize).rev() {
            let up_w = if i == 0 { width } else { down_textures[i - 1].0.width() };
            let up_h = if i == 0 { height } else { down_textures[i - 1].0.height() };

            let (tex, view) = self.create_render_texture(up_w, up_h, &format!("blur up {i}"));

            self.run_pass(&self.up_pipeline, &down_textures[i].2, &view);

            up_textures.push((tex, view));

            let bg = self.make_bind_group(&up_textures.last().unwrap().1);
            if i > 0 {
                // Feed this upsample's result into the next iteration's input slot
                // so the upsample chain actually chains (Kawase dual-filter).
                down_textures[i - 1].2 = bg;
            } else {
                let elapsed = start.elapsed();
                tracing::info!("blur done: {:.2}ms", elapsed.as_secs_f64() * 1000.0);
                return bg;
            }
        }

        // Fallback: if no blur passes, return source
        source_bg
    }

    fn create_render_texture(&self, w: u32, h: u32, label: &str) -> (Texture, TextureView) {
        let tex = self.device.create_texture(&TextureDescriptor {
            label: Some(label),
            size: Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Bgra8UnormSrgb,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = tex.create_view(&TextureViewDescriptor::default());
        (tex, view)
    }

    fn make_bind_group(&self, view: &TextureView) -> BindGroup {
        self.device.create_bind_group(&BindGroupDescriptor {
            label: Some("blur bind group"),
            layout: &self.bgl,
            entries: &[
                BindGroupEntry { binding: 0, resource: BindingResource::TextureView(view) },
                BindGroupEntry { binding: 1, resource: BindingResource::Sampler(&self.sampler) },
            ],
        })
    }

    fn run_pass(&self, pipeline: &RenderPipeline, bind_group: &BindGroup, view: &TextureView) {
        let mut encoder = self.device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("blur pass"),
        });

        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("blur render pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLACK),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }
}
