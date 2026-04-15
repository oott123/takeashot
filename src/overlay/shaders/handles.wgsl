struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

struct Vertex {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs(in: Vertex) -> VOut {
    var out: VOut;
    out.pos = vec4(in.position, 0.0, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs(in: VOut) -> @location(0) vec4<f32> {
    return in.color;
}
