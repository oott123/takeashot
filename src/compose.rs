use anyhow::{Context, Result};
use image::RgbaImage;
use wgpu::*;

use crate::annotation::AnnotationState;
use crate::annotation::render::{tessellate_annotations, tessellate_mosaic_quads};
use crate::blur::DualBlur;
use crate::capture::CapturedScreen;
use crate::geom::Rect;
use crate::overlay::renderer::{ColoredVertex, Gpu, SelectionUniform, TexturedVertex};

/// Per-output info needed for composition. Extracted from overlay state to avoid
/// borrowing the entire OverlayState during GPU work.
pub struct OutputInfo {
    pub output_name: Option<String>,
    pub output_pos: (i32, i32),
    pub width: u32,
    pub height: u32,
    pub scale_factor: i32,
    pub bg_bind_group: BindGroup,
}

/// Render the confirmed selection (screenshot + annotations, no dim overlay, no handles)
/// and composite all outputs into a single RGBA image cropped to the selection rect.
///
/// `blur_passes` controls the mosaic blur strength — match the value that was shown
/// in the overlay so the exported image looks like what the user saw.
pub fn compose_selection(
    gpu: &Gpu,
    outputs: &[OutputInfo],
    captured: &[CapturedScreen],
    annotations: &AnnotationState,
    selection_rect: Rect,
    blur_passes: u32,
) -> Result<RgbaImage> {
    let device = &gpu.device;

    // Build per-output render tasks: only outputs that intersect the selection.
    let mut tasks: Vec<OutputTask> = Vec::new();
    for o in outputs {
        let output_logical = Rect::new(o.output_pos.0, o.output_pos.1, o.width as i32, o.height as i32);
        let overlap = match selection_rect.intersect(&output_logical) {
            Some(r) => r,
            None => continue,
        };
        let scale = o.scale_factor.max(1) as u32;
        let phys_w = o.width * scale;
        let phys_h = o.height * scale;

        let cap = find_captured(captured, &o.output_name);
        let (bg_bind_group, blurred_bind_group) = if let Some(cap) = cap {
            let uploaded = gpu.upload_bgra_texture(cap.width, cap.height, cap.stride, &cap.bgra);
            let blurred = DualBlur::new(gpu.device.clone(), gpu.queue.clone())
                .context("failed to create DualBlur for compose")?
                .blur(&uploaded.view, cap.width, cap.height, blur_passes);
            (uploaded.bind_group, blurred)
        } else {
            tracing::warn!("no captured screen for output {:?}, skipping", o.output_name);
            continue;
        };

        tasks.push(OutputTask {
            output_pos: o.output_pos,
            logical_size: (o.width, o.height),
            scale,
            phys_size: (phys_w, phys_h),
            bg_bind_group,
            blurred_bind_group,
            overlap,
        });
    }

    if tasks.is_empty() {
        anyhow::bail!("no outputs intersect the selection rect");
    }

    // Selection uniform that makes the screenshot shader show full brightness everywhere.
    let full_selection = SelectionUniform { rect: [0.0, 0.0, 1.0, 1.0] };
    let sel_uniform_buf = device.create_buffer(&BufferDescriptor {
        label: Some("compose selection uniform"),
        size: std::mem::size_of::<SelectionUniform>() as u64,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    gpu.queue.write_buffer(&sel_uniform_buf, 0, bytemuck::bytes_of(&full_selection));
    let sel_bind_group = gpu.create_selection_bind_group(&sel_uniform_buf);

    // Render each output offscreen and readback.
    let mut per_output_rgba: Vec<(OutputTask, Vec<u8>, u32)> = Vec::new();
    for task in &tasks {
        let (phys_w, phys_h) = task.phys_size;
        let (logic_w, logic_h) = task.logical_size;

        let offscreen = device.create_texture(&TextureDescriptor {
            label: Some("compose offscreen"),
            size: Extent3d { width: phys_w, height: phys_h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Bgra8UnormSrgb,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = offscreen.create_view(&TextureViewDescriptor::default());

        // Tessellate annotations for this output
        let output_rect = Rect::new(task.output_pos.0, task.output_pos.1, logic_w as i32, logic_h as i32);
        let ann_verts = tessellate_annotations(
            annotations.annotations(),
            annotations.drawing_shape(),
            annotations.drawing_transform(),
            None,
            &[],
            None,
            output_rect,
            task.scale as i32,
            (phys_w, phys_h),
        );
        let ann_vert_count = ann_verts.len().min(Gpu::MAX_ANNOTATION_VERTICES as usize) as u32;

        let ann_vbuf = if ann_vert_count > 0 {
            let buf = device.create_buffer(&BufferDescriptor {
                label: Some("compose annotation vbuf"),
                size: Gpu::MAX_ANNOTATION_VERTICES * std::mem::size_of::<ColoredVertex>() as u64,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bytes = ann_vert_count as usize * std::mem::size_of::<ColoredVertex>();
            gpu.queue.write_buffer(&buf, 0, &bytemuck::cast_slice(&ann_verts)[..bytes]);
            Some(buf)
        } else {
            None
        };

        // Tessellate mosaic quads for this output
        let mos_verts = tessellate_mosaic_quads(
            annotations.annotations(),
            annotations.drawing_shape(),
            annotations.drawing_transform(),
            output_rect,
            task.scale as i32,
            (phys_w, phys_h),
        );
        let mos_vert_count = mos_verts.len().min(Gpu::MAX_MOSAIC_VERTICES as usize) as u32;

        let mos_vbuf = if mos_vert_count > 0 {
            let buf = device.create_buffer(&BufferDescriptor {
                label: Some("compose mosaic vbuf"),
                size: Gpu::MAX_MOSAIC_VERTICES * std::mem::size_of::<TexturedVertex>() as u64,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bytes = mos_vert_count as usize * std::mem::size_of::<TexturedVertex>();
            gpu.queue.write_buffer(&buf, 0, &bytemuck::cast_slice(&mos_verts)[..bytes]);
            Some(buf)
        } else {
            None
        };

        // Render: screenshot + mosaic + annotations (no selection handles)
        let ann_arg = ann_vbuf.as_ref().map(|buf| (buf, ann_vert_count));
        let mos_arg: Option<(&BindGroup, &Buffer, u32)> = mos_vbuf.as_ref()
            .map(|buf| (&task.blurred_bind_group, buf, mos_vert_count));
        gpu.render_into(&view, &task.bg_bind_group, &sel_bind_group, None, ann_arg, mos_arg);

        // Readback via staging buffer
        let row_bytes = phys_w * 4;
        let bytes_per_row = align_row_bytes(row_bytes);
        let staging_size = bytes_per_row as u64 * phys_h as u64;

        let staging = device.create_buffer(&BufferDescriptor {
            label: Some("compose staging"),
            size: staging_size,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("compose copy"),
        });
        encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &offscreen,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: &staging,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(phys_h),
                },
            },
            Extent3d { width: phys_w, height: phys_h, depth_or_array_layers: 1 },
        );
        gpu.queue.submit(std::iter::once(encoder.finish()));

        // Map and read
        let buf_slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buf_slice.map_async(MapMode::Read, move |result| { let _ = tx.send(result); });
        device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None })
            .context("device poll during compose readback")?;
        rx.recv().context("readback channel closed")?.context("readback map failed")?;

        let data = buf_slice.get_mapped_range();
        let mut pixels = Vec::with_capacity((phys_w * phys_h * 4) as usize);
        for y in 0..phys_h {
            let row_start = y as usize * bytes_per_row as usize;
            let row_end = row_start + row_bytes as usize;
            pixels.extend_from_slice(&data[row_start..row_end]);
        }
        drop(data);
        staging.unmap();

        per_output_rgba.push((task.clone(), pixels, row_bytes));
    }

    // Composite all outputs into one image.
    // Final image size: selection rect in logical pixels × max scale factor.
    let max_scale = tasks.iter().map(|t| t.scale).max().unwrap_or(1);
    let final_phys_w = (selection_rect.w as u32 * max_scale) as usize;
    let final_phys_h = (selection_rect.h as u32 * max_scale) as usize;

    let mut final_img = vec![0u8; final_phys_w * final_phys_h * 4];

    for (task, pixels, _row_bytes) in &per_output_rgba {
        let (phys_w, _phys_h) = task.phys_size;
        let scale = task.scale;
        let overlap = &task.overlap;

        let local_overlap = overlap.translate(-task.output_pos.0, -task.output_pos.1);

        let src_x = local_overlap.x as u32 * scale;
        let src_y = local_overlap.y as u32 * scale;
        let src_w = local_overlap.w as u32 * scale;
        let src_h = local_overlap.h as u32 * scale;

        let dst_x = (overlap.x - selection_rect.x) as u32 * max_scale;
        let dst_y = (overlap.y - selection_rect.y) as u32 * max_scale;

        if scale == max_scale {
            copy_region(
                pixels, phys_w * 4, src_x, src_y, src_w, src_h,
                &mut final_img, final_phys_w as u32 * 4, dst_x, dst_y,
            );
        } else {
            scale_region(
                pixels, phys_w * 4, src_x, src_y, src_w, src_h,
                &mut final_img, final_phys_w as u32 * 4, dst_x, dst_y,
                scale, max_scale,
            );
        }
    }

    // BGRA → RGBA
    for px in final_img.chunks_exact_mut(4) {
        px.swap(0, 2);
    }

    RgbaImage::from_raw(final_phys_w as u32, final_phys_h as u32, final_img)
        .context("failed to create final RgbaImage")
}

#[derive(Clone)]
struct OutputTask {
    output_pos: (i32, i32),
    logical_size: (u32, u32),
    scale: u32,
    phys_size: (u32, u32),
    bg_bind_group: BindGroup,
    blurred_bind_group: BindGroup,
    overlap: Rect,
}

/// Align row bytes to COPY_BYTES_PER_ROW_ALIGNMENT (256).
fn align_row_bytes(row_bytes: u32) -> u32 {
    const ALIGN: u32 = COPY_BYTES_PER_ROW_ALIGNMENT;
    ((row_bytes + ALIGN - 1) / ALIGN) * ALIGN
}

fn copy_region(
    src: &[u8], src_stride: u32,
    src_x: u32, src_y: u32, src_w: u32, src_h: u32,
    dst: &mut [u8], dst_stride: u32,
    dst_x: u32, dst_y: u32,
) {
    let bpp = 4u32;
    for y in 0..src_h {
        let s_off = ((src_y + y) * src_stride + src_x * bpp) as usize;
        let d_off = ((dst_y + y) * dst_stride + dst_x * bpp) as usize;
        let len = (src_w * bpp) as usize;
        if s_off + len <= src.len() && d_off + len <= dst.len() {
            dst[d_off..d_off + len].copy_from_slice(&src[s_off..s_off + len]);
        }
    }
}

fn scale_region(
    src: &[u8], src_stride: u32,
    src_x: u32, src_y: u32, src_w: u32, src_h: u32,
    dst: &mut [u8], dst_stride: u32,
    dst_x: u32, dst_y: u32,
    src_scale: u32, dst_scale: u32,
) {
    let bpp = 4u32;
    let logical_w = src_w / src_scale;
    let logical_h = src_h / src_scale;
    let dst_w = logical_w * dst_scale;
    let dst_h = logical_h * dst_scale;

    for y in 0..dst_h {
        let src_y_logical = y / dst_scale;
        let src_y_phys = src_y * src_scale + src_y_logical * src_scale;
        for x in 0..dst_w {
            let src_x_logical = x / dst_scale;
            let src_x_phys = src_x * src_scale + src_x_logical * src_scale;
            let s_off = (src_y_phys * src_stride + src_x_phys * bpp) as usize;
            let d_off = ((dst_y + y) * dst_stride + (dst_x + x) * bpp) as usize;
            if s_off + 4 <= src.len() && d_off + 4 <= dst.len() {
                dst[d_off..d_off + 4].copy_from_slice(&src[s_off..s_off + 4]);
            }
        }
    }
}

fn find_captured<'a>(captured: &'a [CapturedScreen], output_name: &Option<String>) -> Option<&'a CapturedScreen> {
    if let Some(name) = output_name {
        captured.iter().find(|c| c.name == *name)
    } else {
        captured.first()
    }
}
