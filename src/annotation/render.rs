use crate::annotation::{Annotation, EditHandle, EditHandlePos, Shape};
use crate::geom::Rect;
use crate::overlay::renderer::ColoredVertex;
use glam::{Affine2, Vec2};
use lyon_path::Path;
use lyon_tessellation::{StrokeOptions, StrokeTessellator, BuffersBuilder, VertexBuffers};

/// Tessellate annotations into vertex data for the handles pipeline.
///
/// - `annotations`: slice of annotations to render
/// - `drawing_shape`: the in-progress drawing shape (if any)
/// - `drawing_transform`: transform for the in-progress drawing
/// - `drawing_color`: color for the in-progress drawing
/// - `edit_handles`: edit handles to render for the selected annotation
/// - `selected_bounds`: bounding box of the selected annotation (for dashed border)
/// - `output_rect`: global logical rect of this output (origin + logical size)
/// - `scale_factor`: output scale factor (logical → physical)
/// - `surface_size`: physical pixel size of the wgpu surface
///
/// Returns vertices in clip-space NDC coordinates, suitable for the handles pipeline.
pub fn tessellate_annotations(
    annotations: &[Annotation],
    drawing_shape: Option<&Shape>,
    drawing_transform: Option<Affine2>,
    drawing_color: Option<[f32; 4]>,
    edit_handles: &[EditHandlePos],
    selected_bounds: Option<Rect>,
    output_rect: Rect,
    scale_factor: i32,
    surface_size: (u32, u32),
) -> Vec<ColoredVertex> {
    let sw = surface_size.0 as f32;
    let sh = surface_size.1 as f32;
    if sw <= 0.0 || sh <= 0.0 {
        return Vec::new();
    }

    let ox = output_rect.x as f32;
    let oy = output_rect.y as f32;
    let sf = scale_factor as f32;

    let mut vertices = Vec::new();

    // Tessellate finalized annotations
    for ann in annotations {
        tessellate_one(&ann.shape, ann.transform, ann.color, ann.stroke_width, ox, oy, sf, sw, sh, &mut vertices);
    }

    // Tessellate in-progress drawing
    if let Some(shape) = drawing_shape {
        let transform = drawing_transform.unwrap_or(glam::Affine2::IDENTITY);
        let color = drawing_color.unwrap_or([1.0, 0.2, 0.2, 1.0]);
        tessellate_one(shape, transform, color, 3.0, ox, oy, sf, sw, sh, &mut vertices);
    }

    // Dashed border around selected annotation (alternating white and gray)
    if let Some(bounds) = selected_bounds {
        let tl = global_to_ndc(Vec2::new(bounds.x as f32, bounds.y as f32), ox, oy, sf, sw, sh);
        let br = global_to_ndc(Vec2::new((bounds.x + bounds.w) as f32, (bounds.y + bounds.h) as f32), ox, oy, sf, sw, sh);
        const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
        const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
        dashed_rect(tl[0], tl[1], br[0], br[1], sf, sw, sh, WHITE, BLACK, &mut vertices);
    }

    // Render edit handles with dark outlines
    const HANDLE_OUTLINE_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 0.8];
    const OUTLINE_LOGICAL: f32 = 1.0; // outline width per side in logical pixels
    let outline_dx = OUTLINE_LOGICAL * sf / sw * 2.0;
    let outline_dy = OUTLINE_LOGICAL * sf / sh * 2.0;

    for hp in edit_handles {
        let ndc = global_to_ndc(hp.pos, ox, oy, sf, sw, sh);
        let handle_color: [f32; 4] = match hp.kind {
            EditHandle::Corner(_) => [1.0, 1.0, 1.0, 1.0],  // white
            EditHandle::Rotation => [0.27, 0.53, 0.87, 1.0], // KDE blue
        };
        let hs_ndc = 4.0 * sf / sw * 2.0; // 4 logical-pixel half-size in NDC
        let vs_ndc = 4.0 * sf / sh * 2.0;
        let (cx, cy) = (ndc[0], ndc[1]);

        match hp.kind {
            EditHandle::Corner(_) => {
                // Outline (larger square)
                vertices.extend_from_slice(&quad(
                    cx - hs_ndc - outline_dx, cy - vs_ndc - outline_dy,
                    cx + hs_ndc + outline_dx, cy + vs_ndc + outline_dy,
                    HANDLE_OUTLINE_COLOR,
                ));
                // Fill
                vertices.extend_from_slice(&quad(
                    cx - hs_ndc, cy - vs_ndc,
                    cx + hs_ndc, cy + vs_ndc,
                    handle_color,
                ));
            }
            EditHandle::Rotation => {
                // Outline diamond
                let ohs = hs_ndc * 1.2 + outline_dx;
                let ovs = vs_ndc * 1.2 + outline_dy;
                vertices.push(ColoredVertex { position: [cx, cy - ovs], color: HANDLE_OUTLINE_COLOR });
                vertices.push(ColoredVertex { position: [cx + ohs, cy], color: HANDLE_OUTLINE_COLOR });
                vertices.push(ColoredVertex { position: [cx, cy + ovs], color: HANDLE_OUTLINE_COLOR });
                vertices.push(ColoredVertex { position: [cx, cy + ovs], color: HANDLE_OUTLINE_COLOR });
                vertices.push(ColoredVertex { position: [cx - ohs, cy], color: HANDLE_OUTLINE_COLOR });
                vertices.push(ColoredVertex { position: [cx, cy - ovs], color: HANDLE_OUTLINE_COLOR });
                // Fill diamond
                let hs = hs_ndc * 1.2;
                let vs = vs_ndc * 1.2;
                vertices.push(ColoredVertex { position: [cx, cy - vs], color: handle_color });
                vertices.push(ColoredVertex { position: [cx + hs, cy], color: handle_color });
                vertices.push(ColoredVertex { position: [cx, cy + vs], color: handle_color });
                vertices.push(ColoredVertex { position: [cx, cy + vs], color: handle_color });
                vertices.push(ColoredVertex { position: [cx - hs, cy], color: handle_color });
                vertices.push(ColoredVertex { position: [cx, cy - vs], color: handle_color });
            }
        }
    }

    vertices
}

/// Convert global logical coordinates to NDC.
fn global_to_ndc(gp: Vec2, ox: f32, oy: f32, sf: f32, sw: f32, sh: f32) -> [f32; 2] {
    let local_x = (gp.x - ox) * sf;
    let local_y = (gp.y - oy) * sf;
    let ndc_x = local_x / sw * 2.0 - 1.0;
    let ndc_y = 1.0 - local_y / sh * 2.0;
    [ndc_x, ndc_y]
}

fn tessellate_one(
    shape: &Shape,
    transform: Affine2,
    color: [f32; 4],
    stroke_width: f32,
    ox: f32,
    oy: f32,
    sf: f32,
    sw: f32,
    sh: f32,
    vertices: &mut Vec<ColoredVertex>,
) {
    let path = match shape_to_path(shape) {
        Some(p) => p,
        None => return,
    };

    let mut mesh: VertexBuffers<AnnotVertex, u16> = VertexBuffers::new();

    let mut tessellator = StrokeTessellator::new();
    let options = StrokeOptions::default().with_line_width(stroke_width);

    if tessellator.tessellate_path(&path, &options, &mut BuffersBuilder::new(&mut mesh, AnnotVertexCtor)).is_err() {
        return;
    }

    // Transform vertices and add to output
    for chunk in mesh.indices.chunks(3) {
        if chunk.len() < 3 { break; }
        let v0 = mesh.vertices[chunk[0] as usize];
        let v1 = mesh.vertices[chunk[1] as usize];
        let v2 = mesh.vertices[chunk[2] as usize];

        for local_pos in [v0.pos, v1.pos, v2.pos] {
            let gp = transform.transform_point2(Vec2::new(local_pos.0, local_pos.1));
            let ndc = global_to_ndc(gp, ox, oy, sf, sw, sh);
            vertices.push(ColoredVertex {
                position: ndc,
                color,
            });
        }
    }
}

/// Intermediate vertex from lyon tessellation (in shape-local coords).
#[derive(Clone, Copy)]
struct AnnotVertex {
    pos: (f32, f32),
}

struct AnnotVertexCtor;

impl lyon_tessellation::StrokeVertexConstructor<AnnotVertex> for AnnotVertexCtor {
    fn new_vertex(&mut self, attr: lyon_tessellation::StrokeVertex) -> AnnotVertex {
        AnnotVertex {
            pos: (attr.position().x, attr.position().y),
        }
    }
}

/// Convert a Shape to a lyon Path.
fn shape_to_path(shape: &Shape) -> Option<Path> {
    let mut builder = Path::builder();
    match shape {
        Shape::Pen { points } => {
            if points.len() < 2 {
                return None;
            }
            builder.begin(lyon_path::math::point(points[0].x, points[0].y));
            for p in &points[1..] {
                builder.line_to(lyon_path::math::point(p.x, p.y));
            }
            builder.end(false);
        }
        Shape::Line { start, end } => {
            builder.begin(lyon_path::math::point(start.x, start.y));
            builder.line_to(lyon_path::math::point(end.x, end.y));
            builder.end(false);
        }
        Shape::Rect { half_extents } => {
            let he = *half_extents;
            builder.begin(lyon_path::math::point(-he.x, -he.y));
            builder.line_to(lyon_path::math::point(he.x, -he.y));
            builder.line_to(lyon_path::math::point(he.x, he.y));
            builder.line_to(lyon_path::math::point(-he.x, he.y));
            builder.close();
        }
        Shape::Ellipse { radii } => {
            let r = *radii;
            let n = 32;
            let angle_step = std::f32::consts::TAU / n as f32;
            builder.begin(lyon_path::math::point(r.x, 0.0));
            for i in 1..=n {
                let angle = angle_step * i as f32;
                let x = r.x * angle.cos();
                let y = r.y * angle.sin();
                builder.line_to(lyon_path::math::point(x, y));
            }
            builder.close();
        }
    }
    Some(builder.build())
}

/// Draw a dashed rectangle in NDC coordinates.
/// Alternates between `color_a` and `color_b` for each dash segment.
fn dashed_rect(
    l: f32, t: f32, r: f32, b: f32,
    sf: f32, sw: f32, sh: f32,
    color_a: [f32; 4],
    color_b: [f32; 4],
    vertices: &mut Vec<ColoredVertex>,
) {
    const DASH_LEN: f32 = 5.0; // logical pixels
    const GAP_LEN: f32 = 2.0; // logical pixels
    const THICKNESS: f32 = 1.0; // logical pixels

    let thickness_x = THICKNESS * sf / sw * 2.0;
    let thickness_y = THICKNESS * sf / sh * 2.0;

    // Compute total perimeter to determine dash index offset for continuity
    let w = r - l;
    let h = b - t;
    let top_len = w;
    let right_len = h - thickness_y * 2.0;
    let bottom_len = w;
    let _left_len = h - thickness_y * 2.0;
    let period_ndc_h = (DASH_LEN + GAP_LEN) * sf / sw * 2.0;
    let period_ndc_v = (DASH_LEN + GAP_LEN) * sf / sh * 2.0;

    // Count how many full periods fit on each preceding edge to offset dash index
    let top_periods = (top_len / period_ndc_h).floor() as usize;
    let right_periods = (right_len / period_ndc_v).floor() as usize;
    let bottom_periods = (bottom_len / period_ndc_h).floor() as usize;

    // Top edge
    dashed_edge(l, t, r, t, DASH_LEN, GAP_LEN, 0.0, thickness_y, sf, sw, color_a, color_b, 0, vertices);
    // Right edge
    dashed_edge(r - thickness_x, t + thickness_y, r - thickness_x, b - thickness_y, DASH_LEN, GAP_LEN, thickness_x, 0.0, sf, sh, color_a, color_b, top_periods, vertices);
    // Bottom edge (right→left to continue the pattern)
    dashed_edge(r, b - thickness_y, l, b - thickness_y, DASH_LEN, GAP_LEN, 0.0, thickness_y, sf, sw, color_a, color_b, top_periods + right_periods, vertices);
    // Left edge (bottom→top)
    dashed_edge(l, b - thickness_y, l, t + thickness_y, DASH_LEN, GAP_LEN, thickness_x, 0.0, sf, sh, color_a, color_b, top_periods + right_periods + bottom_periods, vertices);
}

/// Draw dashes along one edge, alternating colors.
/// `dash_index_offset` is used to maintain color alternation continuity across edges.
fn dashed_edge(
    x0: f32, y0: f32, x1: f32, y1: f32,
    dash_len: f32, gap_len: f32,
    dx: f32, dy: f32,
    sf: f32, s_dim: f32,
    color_a: [f32; 4],
    color_b: [f32; 4],
    dash_index_offset: usize,
    vertices: &mut Vec<ColoredVertex>,
) {
    let edge_len_ndc = ((x1 - x0).hypot(y1 - y0)).abs();
    let period_ndc = (dash_len + gap_len) * sf / s_dim * 2.0;
    let dash_ndc = dash_len * sf / s_dim * 2.0;

    if edge_len_ndc <= 0.0 {
        return;
    }
    if edge_len_ndc < dash_ndc {
        let color = if dash_index_offset % 2 == 0 { color_a } else { color_b };
        vertices.extend_from_slice(&quad(x0, y0, x1 + dx, y1 + dy, color));
        return;
    }

    let dir_x = (x1 - x0) / edge_len_ndc;
    let dir_y = (y1 - y0) / edge_len_ndc;
    let mut pos = 0.0f32;
    let mut dash_idx = dash_index_offset;

    while pos < edge_len_ndc {
        let dash_start = pos;
        let dash_end = (pos + dash_ndc).min(edge_len_ndc);
        let sx = x0 + dir_x * dash_start;
        let sy = y0 + dir_y * dash_start;
        let ex = x0 + dir_x * dash_end;
        let ey = y0 + dir_y * dash_end;
        let color = if dash_idx % 2 == 0 { color_a } else { color_b };
        vertices.extend_from_slice(&quad(sx, sy, ex + dx, ey + dy, color));
        pos += period_ndc;
        dash_idx += 1;
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
