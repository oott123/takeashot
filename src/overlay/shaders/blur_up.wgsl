@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VOut {
    var positions = array<vec2<f32>, 3>(
        vec2(-1.0, -1.0),
        vec2( 3.0, -1.0),
        vec2(-1.0,  3.0),
    );
    var uvs = array<vec2<f32>, 3>(
        vec2(0.0, 0.0),
        vec2(2.0, 0.0),
        vec2(0.0, 2.0),
    );
    var out: VOut;
    out.pos = vec4(positions[vi], 0.0, 1.0);
    out.uv  = uvs[vi];
    return out;
}

@fragment
fn fs(in: VOut) -> @location(0) vec4<f32> {
    // Kawase / dual-filter upsample (Bjørge 2015).
    // 8-tap ring at half-pixel offsets of the (lower-res) input texture.
    let hp = 0.5 / vec2<f32>(textureDimensions(tex));

    var s = textureSample(tex, samp, in.uv + vec2(-hp.x * 2.0, 0.0));
    s += textureSample(tex, samp, in.uv + vec2(-hp.x,  hp.y)) * 2.0;
    s += textureSample(tex, samp, in.uv + vec2( 0.0,  hp.y * 2.0));
    s += textureSample(tex, samp, in.uv + vec2( hp.x,  hp.y)) * 2.0;
    s += textureSample(tex, samp, in.uv + vec2( hp.x * 2.0, 0.0));
    s += textureSample(tex, samp, in.uv + vec2( hp.x, -hp.y)) * 2.0;
    s += textureSample(tex, samp, in.uv + vec2( 0.0, -hp.y * 2.0));
    s += textureSample(tex, samp, in.uv + vec2(-hp.x, -hp.y)) * 2.0;
    return s / 12.0;
}
