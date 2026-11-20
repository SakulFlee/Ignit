struct DepthRange {
    min_depth: atomic<u32>,
    max_depth: atomic<u32>,
}

@group(0) @binding(0) var source: texture_depth_2d;
@group(0) @binding(1) var<storage, read_write> range: DepthRange;

const GRID: u32 = 256u;

@compute @workgroup_size(8, 8)
fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
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
