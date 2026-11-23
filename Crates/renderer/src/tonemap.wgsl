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

@group(0) @binding(0) var hdr_texture: texture_2d<f32>;
@group(0) @binding(1) var hdr_sampler: sampler;
@group(0) @binding(2) var<uniform> params: TonemapParams;
@group(0) @binding(3) var scene_depth: texture_depth_2d;
@group(0) @binding(5) var integrated: texture_3d<f32>;
@group(0) @binding(6) var integrated_sampler: sampler;
@group(0) @binding(7) var bloom_texture: texture_2d<f32>;
// Light glare is rendered into a half-resolution target by glare.wgsl.
@group(0) @binding(8) var glare_texture: texture_2d<f32>;

// Only the header is read here, for mapping pixels into the scene depth
// texture. The light list is read by glare.wgsl.
struct GlareParams {
    view_projection: mat4x4<f32>,
    camera_position: vec4<f32>,
    // xy = tonemap target size, z = active light count, w = glare radius.
    viewport: vec4<f32>,
    // x = near plane, y = far plane.
    camera_params: vec4<f32>,
    // xy = scene depth texture size. May be smaller than the target when
    // render scaling is active.
    depth_resolution: vec4<f32>,
};

@group(0) @binding(4) var<uniform> glare: GlareParams;

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

/// Maps an output pixel into the scene depth texture. Render scaling and AA
/// can make the tonemap target larger than the depth target.
fn depth_coordinate(pixel: vec2<i32>) -> vec2<i32> {
    let depth_size = max(glare.depth_resolution.xy, vec2<f32>(1.0));
    let output_size = max(glare.viewport.xy, vec2<f32>(1.0));
    let coordinate = clamp(
        (vec2<f32>(pixel) + vec2<f32>(0.5)) * depth_size / output_size,
        vec2<f32>(0.0),
        depth_size - vec2<f32>(1.0),
    );
    return vec2<i32>(coordinate);
}

fn scene_depth_at(pixel: vec2<i32>) -> f32 {
    return textureLoad(scene_depth, depth_coordinate(pixel), 0);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(in.position.xy);
    var base = textureSample(hdr_texture, hdr_sampler, in.uv).rgb;
    var transmittance = 1.0;

    // Fold the old volumetric composite into this pass: sample the
    // pre-integrated froxel column at the scene depth, then attenuate the
    // scene and add the in-scattering before tonemapping.
    if (params.volumetric_enabled != 0u) {
        let near = glare.camera_params.x;
        let far = glare.camera_params.y;
        let depth = scene_depth_at(pixel);
        let scene_view_depth =
            (near * far) / max(far - depth * (far - near), 1.0e-6);
        let t_max = clamp(
            scene_view_depth / max(params.volumetric_range, 0.1),
            0.0,
            1.0,
        );
        // The froxel volume uses the pre-flip UV convention of the old
        // fullscreen composite pass.
        let volume_uv = vec2<f32>(in.uv.x, 1.0 - in.uv.y);
        let prefix = textureSampleLevel(
            integrated,
            integrated_sampler,
            vec3<f32>(volume_uv, t_max),
            0.0,
        );
        base = base * prefix.a + prefix.rgb;
        transmittance = prefix.a;
    }

    // The bloom texture is thresholded and blurred at half resolution. Dim
    // it by the fog transmittance so it stays behind the fog.
    let bloom =
        textureSample(bloom_texture, hdr_sampler, in.uv).rgb
        * params.bloom_intensity
        * transmittance;
    let light_glare = textureSample(glare_texture, hdr_sampler, in.uv).rgb;
    let color = (base + bloom + light_glare) * params.exposure;

    // ACES filmic curve.
    let mapped = clamp(
        (color * (2.51 * color + 0.03)) /
        (color * (2.43 * color + 0.59) + 0.14),
        vec3<f32>(0.0),
        vec3<f32>(1.0)
    );

    // A post-tonemap contrast curve restores separation after fog and tone
    // mapping compress the scene. 1.0 is neutral.
    let contrasted = clamp(
        (mapped - vec3<f32>(0.5)) * params.contrast + vec3<f32>(0.5),
        vec3<f32>(0.0),
        vec3<f32>(1.0),
    );

    // Linear targets need the transfer function here; sRGB targets apply it
    // in hardware, so gamma is 1.0 in that case.
    let corrected = pow(contrasted, vec3<f32>(params.gamma));
    return vec4<f32>(corrected, 1.0);
}
