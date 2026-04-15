@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(1) @binding(0) var<uniform> sel: vec4<f32>;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) pixel_pos: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VOut {
    // Full-screen triangle (3 vertices covering the clip rect)
    var positions = array<vec2<f32>, 3>(
        vec2(-1.0, -1.0),
        vec2( 3.0, -1.0),
        vec2(-1.0,  3.0),
    );
    var uvs = array<vec2<f32>, 3>(
        vec2(0.0, 1.0),
        vec2(2.0, 1.0),
        vec2(0.0, -1.0),
    );
    var out: VOut;
    out.pos = vec4(positions[vi], 0.0, 1.0);
    out.uv  = uvs[vi];
    out.pixel_pos = positions[vi];
    return out;
}

@fragment
fn fs(in: VOut) -> @location(0) vec4<f32> {
    let color = textureSample(tex, samp, in.uv);

    // sel = vec4(x, y, w, h) in [0,1] fraction of surface.
    // If w <= 0 or h <= 0, no selection is active → dim everything.
    if (sel.z <= 0.0 || sel.w <= 0.0) {
        return vec4(color.rgb * 0.5, 1.0);
    }

    // Convert clip coords [-1,1] to [0,1]
    let px = (in.pixel_pos.x + 1.0) * 0.5;
    let py = (1.0 - in.pixel_pos.y) * 0.5;

    let inside = px >= sel.x && px < sel.x + sel.z
              && py >= sel.y && py < sel.y + sel.w;

    if (inside) {
        return vec4(color.rgb, 1.0);
    } else {
        return vec4(color.rgb * 0.5, 1.0);
    }
}
