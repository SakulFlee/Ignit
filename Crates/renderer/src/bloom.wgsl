// Downsampled bloom: threshold the scene, then blur at half resolution.
//
// The tonemap adds the blurred texture instead of taking twelve full-res
// samples per pixel. Bloom is low-frequency, so half resolution is enough.

struct TonemapParams {
    exposure: f32,
    gamma: f32,
    bloom_threshold: f32,
    bloom_intensity: f32,
    contrast: f32,
    glare_stars: u32,
    glare_star_arms: u32,
    glare_star_inner: f32,
    glare_star_outer: f32,
    glare_star_length: f32,
    volumetric_enabled: u32,
    volumetric_range: f32,
    _padding: vec2<f32>,
};

@group(0) @binding(0) var<uniform> params: TonemapParams;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var source_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) id: u32) -> VertexOutput {
    let uv = vec2<f32>(
        f32((id << 1u) & 2u),
        f32(id & 2u),
    );

    var out: VertexOutput;
    out.position = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(uv.x, 1.0 - uv.y);
    return out;
}

@fragment
fn fs_bright(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(source, source_sampler, in.uv).rgb;
    let bright = max(color - vec3<f32>(params.bloom_threshold), vec3<f32>(0.0));
    return vec4<f32>(bright, 1.0);
}

@fragment
fn fs_blur(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(source));
    var sum = vec3<f32>(0.0);
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel;
            sum += textureSample(source, source_sampler, in.uv + offset).rgb;
        }
    }
    return vec4<f32>(sum / 9.0, 1.0);
}
