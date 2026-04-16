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

    // Render edit handles as small squares
    for hp in edit_handles {
        let ndc = global_to_ndc(hp.pos, ox, oy, sf, sw, sh);
        let handle_color: [f32; 4] = match hp.kind {
            EditHandle::Corner(_) => [1.0, 1.0, 1.0, 1.0],  // white
            EditHandle::Rotation => [0.27, 0.53, 0.87, 1.0], // KDE blue
        };
        let hs_ndc = 4.0 / sw * 2.0; // ~4px half-size in NDC
        let vs_ndc = 4.0 / sh * 2.0;
        let (cx, cy) = (ndc[0], ndc[1]);

        match hp.kind {
            EditHandle::Corner(_) => {
                // Square handle
                vertices.extend_from_slice(&quad(
                    cx - hs_ndc, cy - vs_ndc,
                    cx + hs_ndc, cy + vs_ndc,
                    handle_color,
                ));
            }
            EditHandle::Rotation => {
                // Diamond handle
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
