// PBR G-buffer fragment entrypoint.
// Bindings and the vertex entrypoint come from the node library.
//
// Target 1 packs the octahedral normal (RG) with roughness (B) and metallic
// (A) into RGBA8. Target 2 keeps the emissive colour in RGBA16F so bright
// emitters stay HDR.

struct GbufferOutput {
    @location(0) albedo_ao: vec4<f32>,
    @location(1) normal_material: vec4<f32>,
    @location(2) emissive: vec4<f32>,
}

@fragment
fn entrypoint_fragment_gbuffer(in: FragmentData) -> GbufferOutput {
    let surface_data = surface(in);

    if (surface_data.alpha < pbr_factors.alpha_cutoff) {
        discard;
    }

    var out: GbufferOutput;
    out.albedo_ao = vec4<f32>(surface_data.albedo, surface_data.occlusion);
    out.normal_material = vec4<f32>(
        octahedral_encode(normalize(surface_data.normal)),
        surface_data.roughness,
        surface_data.metallic,
    );
    out.emissive = vec4<f32>(surface_data.emissive, 1.0);
    return out;
}
