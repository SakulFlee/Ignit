// Material surface: the data a material provides before lighting.
//
// Forward, G-buffer and shadow variants all consume this so custom materials
// do not need to reimplement lighting or target packing.

// Keep in sync with `ignit_shader_pbr::MATERIAL_TEXTURE_*`.
const HAS_ALBEDO_TEXTURE: u32 = 1u;
const HAS_NORMAL_TEXTURE: u32 = 2u;
const HAS_METALLIC_TEXTURE: u32 = 4u;
const HAS_ROUGHNESS_TEXTURE: u32 = 8u;
const HAS_OCCLUSION_TEXTURE: u32 = 16u;
const HAS_EMISSIVE_TEXTURE: u32 = 32u;

struct Surface {
    albedo: vec3<f32>,
    normal: vec3<f32>,
    roughness: f32,
    metallic: f32,
    emissive: vec3<f32>,
    occlusion: f32,
    alpha: f32,
}

fn surface(fragment_data: FragmentData) -> Surface {
    var out: Surface;
    let flags = pbr_factors.flags;

    if ((flags & HAS_NORMAL_TEXTURE) != 0u) {
        out.normal = sample_normal_from_map(fragment_data);
    } else {
        out.normal = normalize(fragment_data.normal);
    }

    if ((flags & HAS_ALBEDO_TEXTURE) != 0u) {
        let albedo_sample = textureSample(
            albedo_texture,
            albedo_sampler,
            fragment_data.uv
        );
        out.albedo = clamp(
            albedo_sample.rgb * pbr_factors.albedo_factor.rgb,
            vec3(0.0),
            vec3(1.0),
        );
        out.alpha = clamp(
            albedo_sample.a * pbr_factors.alpha_factor,
            0.0,
            1.0,
        );
    } else {
        // sRGB textures are decoded by the sampler; procedural colours are
        // uploaded in the same format. Do not decode again here.
        out.albedo = clamp(
            pbr_factors.albedo_factor.rgb,
            vec3(0.0),
            vec3(1.0),
        );
        out.alpha = clamp(pbr_factors.alpha_factor, 0.0, 1.0);
    }

    var metallic_sample = 1.0;
    if ((flags & HAS_METALLIC_TEXTURE) != 0u) {
        metallic_sample = textureSample(
            metallic_texture,
            metallic_sampler,
            fragment_data.uv
        ).r;
    }
    out.metallic = clamp(
        metallic_sample * pbr_factors.metallic_factor,
        0.0,
        1.0,
    );

    var roughness_sample = 1.0;
    if ((flags & HAS_ROUGHNESS_TEXTURE) != 0u) {
        roughness_sample = textureSample(
            roughness_texture,
            roughness_sampler,
            fragment_data.uv
        ).r;
    }
    out.roughness = clamp(
        roughness_sample * pbr_factors.roughness_factor,
        0.045,
        0.9999,
    );

    var occlusion_sample = 1.0;
    if ((flags & HAS_OCCLUSION_TEXTURE) != 0u) {
        occlusion_sample = textureSample(
            occlusion_texture,
            occlusion_sampler,
            fragment_data.uv
        ).r;
    }
    out.occlusion = clamp(occlusion_sample, 0.0, 1.0);

    var emissive_sample = vec3<f32>(0.0);
    if ((flags & HAS_EMISSIVE_TEXTURE) != 0u) {
        emissive_sample = textureSample(
            emissive_texture,
            emissive_sampler,
            fragment_data.uv
        ).rgb;
    }
    out.emissive = clamp(emissive_sample, vec3(0.0), vec3(1.0));

    return out;
}
