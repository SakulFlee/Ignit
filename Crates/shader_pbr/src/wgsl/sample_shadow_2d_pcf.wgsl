fn sample_shadow_2d_pcf(layer: u32, shadow_coord: vec3<f32>, depth: f32) -> f32 {
    let dims = vec2<f32>(textureDimensions(shadow_map_array));
    let texel = 1.0 / dims;
    let clamped_uv = clamp(shadow_coord.xy, texel, vec2<f32>(1.0) - texel);
    let taps = shadow_data.header.y;

    if (taps <= 1u) {
        return textureSampleCompareLevel(
            shadow_map_array, shadow_sampler, clamped_uv, i32(layer), depth
        );
    }

    if (taps <= 4u) {
        var sum4 = 0.0;
        for (var y = 0; y < 2; y++) {
            for (var x = 0; x < 2; x++) {
                let offset = (vec2<f32>(f32(x), f32(y)) - vec2<f32>(0.5)) * texel;
                sum4 += textureSampleCompareLevel(
                    shadow_map_array, shadow_sampler,
                    clamped_uv + offset, i32(layer), depth
                );
            }
        }
        return sum4 / 4.0;
    }

    var sum = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel;
            sum += textureSampleCompareLevel(
                shadow_map_array, shadow_sampler,
                clamped_uv + offset, i32(layer), depth
            );
        }
    }
    return sum / 9.0;
}
