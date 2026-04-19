use crate::annotation::{Annotation, EditHandle, EditHandlePos, OrientedRect, Shape};
use crate::geom::Rect;
use crate::overlay::renderer::{ColoredVertex, TexturedVertex};
use glam::{Affine2, Vec2};
use lyon_path::Path;
use lyon_tessellation::{StrokeOptions, StrokeTessellator, FillTessellator, FillOptions, BuffersBuilder, VertexBuffers};

/// Tessellate annotations into vertex data for the handles pipeline.
///
/// - `annotations`: slice of annotations to render
/// - `drawing_shape`: the in-progress drawing shape (if any)
/// - `drawing_transform`: transform for the in-progress drawing
/// - `drawing_color`: color for the in-progress drawing
/// - `edit_handles`: edit handles to render for the selected annotation
/// - `selected_oriented`: oriented bounding box of the selected annotation (for dashed border)
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
    drawing_stroke_width: f32,
    drawing_filled: bool,
    edit_handles: &[EditHandlePos],
    selected_oriented: Option<OrientedRect>,
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
        tessellate_one(&ann.shape, ann.transform, ann.color, ann.stroke_width, ann.filled, ox, oy, sf, sw, sh, &mut vertices);
    }

    // Tessellate in-progress drawing
    if let Some(shape) = drawing_shape {
        let transform = drawing_transform.unwrap_or(glam::Affine2::IDENTITY);
        let color = drawing_color.unwrap_or([1.0, 0.2, 0.2, 1.0]);
        tessellate_one(shape, transform, color, drawing_stroke_width, drawing_filled, ox, oy, sf, sw, sh, &mut vertices);
    }

    // Dashed border around selected annotation (alternating white and gray)
    if let Some(ob) = selected_oriented {
        let ndc_corners: [[f32; 2]; 4] = ob.corners.map(|c| global_to_ndc(c, ox, oy, sf, sw, sh));
        const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
        const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
        dashed_oriented_rect(&ndc_corners, sf, sw, sh, WHITE, BLACK, &mut vertices);
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
    filled: bool,
    ox: f32,
    oy: f32,
    sf: f32,
    sw: f32,
    sh: f32,
    vertices: &mut Vec<ColoredVertex>,
) {
    // Mosaic shapes are rendered as textured quads, not stroked geometry
    if matches!(shape, Shape::Mosaic { .. }) {
        return;
    }

    let path = match shape_to_path(shape) {
        Some(p) => p,
        None => return,
    };

    let mut mesh: VertexBuffers<AnnotVertex, u16> = VertexBuffers::new();

    let ok = if filled && matches!(shape, Shape::Rect { .. } | Shape::Ellipse { .. }) {
        FillTessellator::new().tessellate_path(
            &path,
            &FillOptions::default(),
            &mut BuffersBuilder::new(&mut mesh, AnnotVertexCtor),
        ).is_ok()
    } else {
        StrokeTessellator::new().tessellate_path(
            &path,
            &StrokeOptions::default().with_line_width(stroke_width),
            &mut BuffersBuilder::new(&mut mesh, AnnotVertexCtor),
        ).is_ok()
    };
    if !ok { return; }

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

impl lyon_tessellation::FillVertexConstructor<AnnotVertex> for AnnotVertexCtor {
    fn new_vertex(&mut self, attr: lyon_tessellation::FillVertex) -> AnnotVertex {
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
        Shape::Rect { half_extents } | Shape::Mosaic { half_extents, .. } => {
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

/// Draw a dashed oriented rectangle in NDC coordinates from 4 corner positions.
/// Corners: [TL, TR, BR, BL].
fn dashed_oriented_rect(
    corners: &[[f32; 2]; 4],
    sf: f32, sw: f32, sh: f32,
    color_a: [f32; 4],
    color_b: [f32; 4],
    vertices: &mut Vec<ColoredVertex>,
) {
    const DASH_LEN: f32 = 5.0;
    const GAP_LEN: f32 = 2.0;
    const THICKNESS: f32 = 1.0; // logical pixels

    // Compute edge lengths for dash index continuity
    let edge_len = |a: &[f32; 2], b: &[f32; 2]| -> f32 {
        ((b[0] - a[0]).hypot(b[1] - a[1])).abs()
    };

    // For each edge, compute the inward perpendicular direction for thickness
    let center_x = (corners[0][0] + corners[2][0]) / 2.0;
    let center_y = (corners[0][1] + corners[2][1]) / 2.0;

    let perp_inward = |a: &[f32; 2], b: &[f32; 2]| -> (f32, f32) {
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let len = dx.hypot(dy);
        if len < 1e-6 { return (0.0, 0.0); }
        // Two candidate perpendiculars: (-dy, dx) and (dy, -dx)
        let mid_x = (a[0] + b[0]) / 2.0;
        let mid_y = (a[1] + b[1]) / 2.0;
        let (nx, ny) = (-dy / len, dx / len);
        // Pick the one pointing toward center
        let dot = nx * (center_x - mid_x) + ny * (center_y - mid_y);
        if dot > 0.0 { (nx, ny) } else { (-nx, -ny) }
    };

    // Convert thickness to NDC scale (use geometric mean of X/Y NDC scales)
    let thickness_ndc_x = THICKNESS * sf / sw * 2.0;
    let thickness_ndc_y = THICKNESS * sf / sh * 2.0;

    let top_len = edge_len(&corners[0], &corners[1]);
    let right_len = edge_len(&corners[1], &corners[2]);
    let bottom_len = edge_len(&corners[2], &corners[3]);

    let period_ndc = (DASH_LEN + GAP_LEN) * sf / sw * 2.0;

    let top_periods = (top_len / period_ndc).floor() as usize;
    let right_periods = (right_len / period_ndc).floor() as usize;
    let bottom_periods = (bottom_len / period_ndc).floor() as usize;

    let edges = [
        (&corners[0], &corners[1], 0),
        (&corners[1], &corners[2], top_periods),
        (&corners[2], &corners[3], top_periods + right_periods),
        (&corners[3], &corners[0], top_periods + right_periods + bottom_periods),
    ];

    for (a, b, offset) in edges {
        let (nx, ny) = perp_inward(a, b);
        let tnx = nx * thickness_ndc_x;
        let tny = ny * thickness_ndc_y;
        dashed_edge(
            a[0], a[1], b[0], b[1],
            DASH_LEN, GAP_LEN, tnx, tny, sf, sw, color_a, color_b, offset, vertices,
        );
    }
}

/// Draw dashes along one edge, alternating colors.
/// `dx`/`dy` is the perpendicular offset for line thickness (both start and end).
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
        vertices.extend_from_slice(&quad_offset(x0, y0, x1, y1, dx, dy, color));
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
        vertices.extend_from_slice(&quad_offset(sx, sy, ex, ey, dx, dy, color));
        pos += period_ndc;
        dash_idx += 1;
    }
}

/// Build 6 vertices (2 triangles) for a filled parallelogram from (x0,y0)→(x1,y1)
/// with perpendicular thickness offset (dx, dy).
fn quad_offset(x0: f32, y0: f32, x1: f32, y1: f32, dx: f32, dy: f32, color: [f32; 4]) -> [ColoredVertex; 6] {
    [
        ColoredVertex { position: [x0, y0], color },
        ColoredVertex { position: [x0 + dx, y0 + dy], color },
        ColoredVertex { position: [x1, y1], color },
        ColoredVertex { position: [x1, y1], color },
        ColoredVertex { position: [x0 + dx, y0 + dy], color },
        ColoredVertex { position: [x1 + dx, y1 + dy], color },
    ]
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

pub struct MosaicQuad {
    pub blur_passes: u32,
    pub vertices: [TexturedVertex; 6],
}

/// Walk z-ordered quads and emit one draw range per maximal run of quads
/// that share a `blur_passes` value, so the mosaic pass issues as few draw
/// calls as possible while preserving user-visible ordering.
pub fn coalesce_mosaic_draws(quads: &[MosaicQuad]) -> Vec<(u32, std::ops::Range<u32>)> {
    let mut draws: Vec<(u32, std::ops::Range<u32>)> = Vec::with_capacity(quads.len());
    for (i, q) in quads.iter().enumerate() {
        let start = (i * 6) as u32;
        let end = start + 6;
        if let Some(last) = draws.last_mut() {
            if last.0 == q.blur_passes && last.1.end == start {
                last.1.end = end;
                continue;
            }
        }
        draws.push((q.blur_passes, start..end));
    }
    draws
}

/// Tessellate mosaic annotations into textured quads for the mosaic pipeline.
///
/// For each `Shape::Mosaic` annotation, generates a screen-aligned quad with
/// UV coordinates mapping into the blurred screenshot texture. Each quad
/// carries its own `blur_passes`, so the caller can draw groups with
/// different blur strengths. Non-mosaic shapes are skipped.
///
/// Quads are returned in z-order (finalized annotations first, in-progress last).
pub fn tessellate_mosaic_quads(
    annotations: &[Annotation],
    drawing_shape: Option<&Shape>,
    drawing_transform: Option<Affine2>,
    output_rect: Rect,
    scale_factor: i32,
    surface_size: (u32, u32),
) -> Vec<MosaicQuad> {
    let sw = surface_size.0 as f32;
    let sh = surface_size.1 as f32;
    if sw <= 0.0 || sh <= 0.0 {
        return Vec::new();
    }

    let ox = output_rect.x as f32;
    let oy = output_rect.y as f32;
    let sf = scale_factor as f32;

    let mut quads = Vec::new();

    // Finalized mosaic annotations
    for ann in annotations {
        if let Shape::Mosaic { blur_passes, .. } = &ann.shape {
            if let Some(verts) = build_mosaic_quad(ann, ox, oy, sf, sw, sh) {
                quads.push(MosaicQuad { blur_passes: *blur_passes, vertices: verts });
            }
        }
    }

    // In-progress mosaic drawing
    if let Some(Shape::Mosaic { blur_passes, .. }) = drawing_shape {
        let transform = drawing_transform.unwrap_or(Affine2::IDENTITY);
        let fake_ann = Annotation {
            shape: drawing_shape.unwrap().clone(),
            transform,
            color: [0.0; 4],
            stroke_width: 0.0,
            filled: false,
        };
        if let Some(verts) = build_mosaic_quad(&fake_ann, ox, oy, sf, sw, sh) {
            quads.push(MosaicQuad { blur_passes: *blur_passes, vertices: verts });
        }
    }

    quads
}

fn build_mosaic_quad(
    ann: &Annotation,
    ox: f32, oy: f32, sf: f32,
    sw: f32, sh: f32,
) -> Option<[TexturedVertex; 6]> {
    let bounds = AnnotationState::annotation_bounds(ann);
    if bounds.is_empty() {
        return None;
    }

    // Convert global logical bounds to per-output physical pixel coords → NDC + UV
    let l = (bounds.x as f32 - ox) * sf;
    let t = (bounds.y as f32 - oy) * sf;
    let r = (bounds.x as f32 + bounds.w as f32 - ox) * sf;
    let b = (bounds.y as f32 + bounds.h as f32 - oy) * sf;

    // NDC
    let nl = l / sw * 2.0 - 1.0;
    let nt = 1.0 - t / sh * 2.0;
    let nr = r / sw * 2.0 - 1.0;
    let nb = 1.0 - b / sh * 2.0;

    // UV (in physical pixel space, normalized to surface size)
    let ul = l / sw;
    let vt = t / sh;
    let ur = r / sw;
    let vb = b / sh;

    // 2 triangles: TL-TR-BL, TR-BR-BL
    Some([
        TexturedVertex { position: [nl, nt], uv: [ul, vt] },
        TexturedVertex { position: [nr, nt], uv: [ur, vt] },
        TexturedVertex { position: [nl, nb], uv: [ul, vb] },
        TexturedVertex { position: [nl, nb], uv: [ul, vb] },
        TexturedVertex { position: [nr, nt], uv: [ur, vt] },
        TexturedVertex { position: [nr, nb], uv: [ur, vb] },
    ])
}

use crate::annotation::AnnotationState;

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(passes: u32) -> MosaicQuad {
        MosaicQuad {
            blur_passes: passes,
            vertices: [TexturedVertex { position: [0.0; 2], uv: [0.0; 2] }; 6],
        }
    }

    #[test]
    fn coalesce_adjacent_same_passes() {
        let quads = [quad(3), quad(3), quad(5), quad(5), quad(3)];
        let draws = coalesce_mosaic_draws(&quads);
        assert_eq!(draws, vec![(3, 0..12), (5, 12..24), (3, 24..30)]);
    }

    #[test]
    fn coalesce_empty_input() {
        assert!(coalesce_mosaic_draws(&[]).is_empty());
    }

    #[test]
    fn coalesce_all_distinct() {
        let quads = [quad(1), quad(2), quad(3)];
        let draws = coalesce_mosaic_draws(&quads);
        assert_eq!(draws, vec![(1, 0..6), (2, 6..12), (3, 12..18)]);
    }
}
