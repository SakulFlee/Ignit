//! # PBR Shader Module
//!
//! PBR-specific WGSL nodes (BRDF orchestration, material data gathering, IBL,
//! shadow sampling) plus the [`PBRMaterialShaderDescriptor`] that assembles a
//! full PBR material shader from the reusable node libraries.

use ignit_shader_preprocessor::{NodeLibrary, ShaderNode};

mod pbr_material_shader;
pub use pbr_material_shader::*;

/// Returns the PBR node library. Register it (alongside
/// [`ignit_shader_math::math_library`] and
/// [`ignit_shader_engine::engine_library`]) with a
/// [`ignit_shader_preprocessor::NodeRegistry`] to make these nodes available
/// to shader builders by name.
pub fn pbr_library() -> NodeLibrary {
    let mut lib = NodeLibrary::new("ignit_shader_pbr");

    // Structs
    lib.add(ShaderNode::new(
        "pbr_factors",
        include_str!("wgsl/pbr_factors.wgsl"),
    ));
    lib.add(ShaderNode::new(
        "light_contribution",
        include_str!("wgsl/light_contribution.wgsl"),
    ));
    lib.add(ShaderNode::new(
        "pbr_data_struct",
        include_str!("wgsl/pbr_data_struct.wgsl"),
    ));
    lib.add(ShaderNode::new(
        "spot_bias_scale",
        include_str!("wgsl/spot_bias_scale.wgsl"),
    ));

    // Functions
    lib.add(
        ShaderNode::new(
            "hdr_tone_map_gamma_correction",
            include_str!("wgsl/hdr_tone_map_gamma_correction.wgsl"),
        )
        .with_deps(["camera_uniform"]),
    );
    lib.add(
        ShaderNode::new(
            "sample_normal_from_map",
            include_str!("wgsl/sample_normal_from_map.wgsl"),
        )
        .with_deps(["fragment_data"]),
    );
    lib.add(
        ShaderNode::new("pbr_bindings", include_str!("wgsl/pbr_bindings.wgsl")).with_deps([
            "camera_uniform",
            "light_struct",
            "shadow_types",
            "pbr_factors",
        ]),
    );
    lib.add(
        ShaderNode::new("pbr_vertex", include_str!("wgsl/pbr_vertex.wgsl")).with_deps([
            "camera_uniform",
            "vertex_data_complex",
            "instance_data",
            "fragment_data",
        ]),
    );
    lib.add(
        ShaderNode::new("pbr_surface", include_str!("wgsl/pbr_surface.wgsl")).with_deps([
            "pbr_factors",
            "sample_normal_from_map",
            "camera_uniform",
            "fragment_data",
        ]),
    );
    lib.add(
        ShaderNode::new("pbr_data", include_str!("wgsl/pbr_data.wgsl")).with_deps([
            "pbr_data_struct",
            "pbr_surface",
            "camera_uniform",
            "fragment_data",
        ]),
    );
    lib.add(
        ShaderNode::new(
            "calculate_light_brdf",
            include_str!("wgsl/calculate_light_brdf.wgsl"),
        )
        .with_deps([
            "light_contribution",
            "pbr_data_struct",
            "light_struct",
            "light_types",
            "distribution_ggx",
            "schlick_smith_ggx",
            "fresnel_schlick",
            "f0_default",
            "pi",
        ]),
    );
    lib.add(
        ShaderNode::new(
            "calculate_ambient_ibl",
            include_str!("wgsl/calculate_ambient_ibl.wgsl"),
        )
        .with_deps([
            "pbr_data_struct",
            "fresnel_schlick_roughness",
            "f0_default",
            "ambient_intensity",
        ]),
    );
    lib.add(
        ShaderNode::new(
            "sample_shadow_2d_pcf",
            include_str!("wgsl/sample_shadow_2d_pcf.wgsl"),
        )
        .with_deps(["shadow_slot"]),
    );
    lib.add(
        ShaderNode::new(
            "compute_shadow_for_light",
            include_str!("wgsl/compute_shadow_for_light.wgsl"),
        )
        .with_deps([
            "light_struct",
            "light_types",
            "shadow_types",
            "shadow_slot",
            "slope_scaled_bias",
            "sample_shadow_2d_pcf",
            "spot_bias_scale",
        ]),
    );
    lib.add(
        ShaderNode::new(
            "calculate_light_contribution",
            include_str!("wgsl/calculate_light_contribution.wgsl"),
        )
        .with_deps([
            "pbr_data_struct",
            "calculate_light_brdf",
            "compute_shadow_for_light",
        ]),
    );

    lib
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ignit_shader_preprocessor::{NodeRegistry, ShaderBuilder};

    use super::pbr_library;

    #[test]
    fn shadow_nodes_assemble_to_valid_wgsl() {
        let mut registry = NodeRegistry::new();
        registry
            .register_library(&ignit_shader_engine::engine_library())
            .expect("register engine nodes");
        registry
            .register_library(&ignit_shader_math::math_library())
            .expect("register math nodes");
        registry
            .register_library(&pbr_library())
            .expect("register PBR nodes");

        let mut builder = ShaderBuilder::new(Arc::new(registry));
        builder
            .add_node("compute_shadow_for_light")
            .expect("resolve shadow nodes");
        builder.add_source(
            r#"
@group(0) @binding(1) var<storage> light_store: array<Light>;
@group(0) @binding(8) var<uniform> shadow_data: ShadowData;
@group(0) @binding(9) var shadow_map_array: texture_depth_2d_array;
@group(0) @binding(10) var shadow_sampler: sampler_comparison;
@group(0) @binding(11) var point_shadow_maps: texture_depth_cube_array;
@group(0) @binding(12) var point_shadow_sampler: sampler_comparison;

@compute @workgroup_size(1)
fn test_shadow_call() {
    _ = compute_shadow_for_light(
        vec3<f32>(0.0),
        0.0,
        0u,
        vec3<f32>(0.0, 1.0, 0.0),
    );
}
"#,
        );

        let source = builder.build();
        let module = wgpu::naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|error| panic!("shadow WGSL failed to parse: {error:?}\n{source}"));
        let mut validator = wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        );
        validator
            .validate(&module)
            .unwrap_or_else(|error| panic!("shadow WGSL failed validation: {error:?}"));
    }
}
