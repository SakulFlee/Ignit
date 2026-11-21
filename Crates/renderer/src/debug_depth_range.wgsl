struct DepthRange {
    min_depth: atomic<u32>,
    max_depth: atomic<u32>,
    smoothed_min: u32,
    smoothed_max: u32,
}

@group(0) @binding(0) var source: texture_depth_2d;
@group(0) @binding(1) var<storage, read_write> range: DepthRange;

const GRID: u32 = 256u;
/// Exponential smoothing applied to the visible depth range each frame.
const SMOOTHING: f32 = 0.12;

@compute @workgroup_size(8, 8)
fn cs_reduce(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= GRID || id.y >= GRID) {
        return;
    }

    let dimensions = vec2<u32>(textureDimensions(source));
    let coordinate = vec2<i32>(
        i32(min(id.x * dimensions.x / GRID, dimensions.x - 1u)),
        i32(min(id.y * dimensions.y / GRID, dimensions.y - 1u)),
    );
    let depth = textureLoad(source, coordinate, 0);
    // Cleared and sky pixels sit at the far plane and would collapse the range.
    if (depth <= 0.0 || depth >= 1.0) {
        return;
    }

    // Depth is a non-negative float, so its bit pattern sorts numerically.
    let bits = bitcast<u32>(depth);
    atomicMin(&range.min_depth, bits);
    atomicMax(&range.max_depth, bits);
}

@compute @workgroup_size(1)
fn cs_smooth() {
    let raw_min = atomicLoad(&range.min_depth);
    let raw_max = atomicLoad(&range.max_depth);
    if (raw_max <= raw_min) {
        return;
    }

    let target_min = bitcast<f32>(raw_min);
    let target_max = bitcast<f32>(raw_max);
    let previous_min = bitcast<f32>(range.smoothed_min);
    let previous_max = bitcast<f32>(range.smoothed_max);
    let initialized = range.smoothed_max > range.smoothed_min;

    let smoothed_min = select(target_min, mix(previous_min, target_min, SMOOTHING), initialized);
    let smoothed_max = select(target_max, mix(previous_max, target_max, SMOOTHING), initialized);

    range.smoothed_min = bitcast<u32>(smoothed_min);
    range.smoothed_max = bitcast<u32>(smoothed_max);
}
