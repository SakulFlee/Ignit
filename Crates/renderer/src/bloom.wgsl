// Downsampled bloom: threshold the scene, then build a blur chain.
//
// The tonemap adds the accumulated half-resolution texture instead of taking
// twelve full-res samples per pixel. Each level is box-filtered on the way
// down and tent-filtered on the way back up, so the glow spans several
// screen-space scales at a fraction of the full-resolution cost.
//
// The upsample pass adds into the larger level with additive blending and
// `fs_bright` pre-scales the chain so the accumulated energy matches a single
// level. `BLOOM_ACCUM_SCALE` assumes the level count and weights below, so
// keep it in sync with the renderer's `BLOOM_MIP_LEVELS`.

const BLOOM_UPSAMPLE_WEIGHT: f32 = 0.5;
// 1 + 1/2 + 1/4 + 1/8 for four levels.
const BLOOM_ACCUM_SCALE: f32 = 1.0 / 1.875;

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
    return vec4<f32>(bright * BLOOM_ACCUM_SCALE, 1.0);
}

@fragment
fn fs_downsample(in: VertexOutput) -> @location(0) vec4<f32> {
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

@fragment
fn fs_upsample(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(source));
    var sum = vec3<f32>(0.0);
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel;
            // Tent weights: 1 at the corners, 2 on the edges, 4 in the middle.
            let weight = f32((2 - abs(x)) * (2 - abs(y)));
            sum += textureSample(source, source_sampler, in.uv + offset).rgb * weight;
        }
    }
    return vec4<f32>(sum / 16.0 * BLOOM_UPSAMPLE_WEIGHT, 1.0);
}
