struct DebugParams {
    target_size: vec2<f32>,
    uv_scale: f32,
    _padding: f32,
}

struct DepthRange {
    min_depth: u32,
    max_depth: u32,
    smoothed_min: u32,
    smoothed_max: u32,
}

@group(0) @binding(0) var source: texture_depth_2d;
@group(0) @binding(1) var<uniform> params: DebugParams;
@group(0) @binding(2) var<storage, read> range: DepthRange;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(positions[index], 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let dimensions = vec2<f32>(textureDimensions(source));
    let uv = position.xy / params.target_size * params.uv_scale;
    let coordinate = vec2<i32>(
        clamp(uv * dimensions, vec2<f32>(0.0), dimensions - vec2<f32>(1.0)),
    );
    let depth = textureLoad(source, coordinate, 0);

    // Invert so near geometry is bright and the far plane is black. When the
    // range pass found visible geometry, stretch the scene's actual depth range
    // across the full ramp instead of drawing everything as near-uniform grey.
    var value = 1.0 - depth;
    if (range.smoothed_max > range.smoothed_min) {
        let min_depth = bitcast<f32>(range.smoothed_min);
        let max_depth = bitcast<f32>(range.smoothed_max);
        let span = max(max_depth - min_depth, 1e-6);
        value = clamp(1.0 - (depth - min_depth) / span, 0.0, 1.0);
    }

    return vec4<f32>(value, value, value, 1.0);
}
