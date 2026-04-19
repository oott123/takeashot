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
    let ts = 5.0 / vec2<f32>(textureDimensions(tex));

    // Wide 13-tap upsample matching downsample kernel
    let c   = textureSample(tex, samp, in.uv);
    let n   = textureSample(tex, samp, in.uv + vec2( 0.0, -ts.y));
    let s   = textureSample(tex, samp, in.uv + vec2( 0.0,  ts.y));
    let e   = textureSample(tex, samp, in.uv + vec2( ts.x,  0.0));
    let w   = textureSample(tex, samp, in.uv + vec2(-ts.x,  0.0));
    let nw  = textureSample(tex, samp, in.uv + vec2(-ts.x, -ts.y));
    let ne  = textureSample(tex, samp, in.uv + vec2( ts.x, -ts.y));
    let se  = textureSample(tex, samp, in.uv + vec2( ts.x,  ts.y));
    let sw  = textureSample(tex, samp, in.uv + vec2(-ts.x,  ts.y));
    let nn  = textureSample(tex, samp, in.uv + vec2( 0.0, -ts.y * 2.0));
    let ss  = textureSample(tex, samp, in.uv + vec2( 0.0,  ts.y * 2.0));
    let ee  = textureSample(tex, samp, in.uv + vec2( ts.x * 2.0,  0.0));
    let ww  = textureSample(tex, samp, in.uv + vec2(-ts.x * 2.0,  0.0));

    return c * 0.16 + (n + s + e + w) * 0.1 + (nw + ne + se + sw) * 0.07 + (nn + ss + ee + ww) * 0.05;
}
