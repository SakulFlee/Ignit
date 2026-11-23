// Octahedral normal encoding for the deferred G-buffer.
//
// The G-buffer stores the world-space surface normal in two channels instead
// of three, which lets normal, roughness, and metallic share one RGBA8 target.

/// Wraps the lower hemisphere around the octahedron so the whole sphere maps
/// into the unit square.
fn octahedral_wrap(value: vec2<f32>) -> vec2<f32> {
    return (vec2<f32>(1.0) - abs(value.yx))
        * vec2<f32>(
            select(-1.0, 1.0, value.x >= 0.0),
            select(-1.0, 1.0, value.y >= 0.0),
        );
}

/// Encodes a unit normal into `[0, 1]` octahedral coordinates.
fn octahedral_encode(normal: vec3<f32>) -> vec2<f32> {
    let l1 = abs(normal.x) + abs(normal.y) + abs(normal.z);
    let unit = normal / max(l1, 1.0e-6);
    var encoded = unit.xy;
    if (unit.z < 0.0) {
        encoded = octahedral_wrap(encoded);
    }
    return encoded * 0.5 + vec2<f32>(0.5);
}

/// Decodes octahedral coordinates back into a unit normal.
fn octahedral_decode(encoded: vec2<f32>) -> vec3<f32> {
    let value = encoded * 2.0 - vec2<f32>(1.0);
    var normal = vec3<f32>(value.x, value.y, 1.0 - abs(value.x) - abs(value.y));
    if (normal.z < 0.0) {
        normal = vec3<f32>(
            (1.0 - abs(normal.y)) * select(-1.0, 1.0, normal.x >= 0.0),
            (1.0 - abs(normal.x)) * select(-1.0, 1.0, normal.y >= 0.0),
            normal.z,
        );
    }
    return normalize(normal);
}
