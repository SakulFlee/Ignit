struct DebugParams {
    target_size: vec2<f32>,
    _padding: vec2<f32>,
}

@group(0) @binding(0) var source: texture_depth_2d;
@group(0) @binding(1) var<uniform> params: DebugParams;

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
    let uv = position.xy / params.target_size;
    let coordinate = vec2<i32>(
        clamp(uv * dimensions, vec2<f32>(0.0), dimensions - vec2<f32>(1.0)),
    );
    let depth = textureLoad(source, coordinate, 0);
    // Invert so near geometry is bright and the far plane is black.
    let value = 1.0 - depth;
    return vec4<f32>(value, value, value, 1.0);
}
