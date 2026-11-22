fn sample_shadow_2d_pcf(
    layer: u32,
    uv: vec2<f32>,
    depth: f32,
    uv_scale: f32,
) -> f32 {
    // The layer is allocated at the full array size; `uv_scale` maps the full
    // light frustum into the smaller viewport it was rasterized at. Taps still
    // step by one physical array texel.
    let texel = 1.0 / vec2<f32>(textureDimensions(shadow_map_array));
    let max_uv = max(vec2<f32>(uv_scale), 2.0 * texel);
    let clamped_uv = clamp(uv, texel, max_uv - texel);
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
