use std::sync::Arc;

use cgmath::Vector3;
use ignit_material_shader::{AlphaMode, MaterialShaderDescriptor, VertexStageLayout};
use ignit_shader_debug::wireframe_descriptor;
use ignit_shader_pbr::{MATERIAL_TEXTURE_OCCLUSION, PBRMaterialShaderDescriptor};
use ignit_texture::{FilterMode, TextureDescriptor, TextureSize};
use wgpu::{TextureUsages, VertexAttribute, VertexBufferLayout, VertexFormat, VertexStepMode};

const DEFAULT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Matches the hardware sRGB decode used by uploaded colour textures.
fn srgb_to_linear(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn mat_texture_usages() -> TextureUsages {
    TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SceneMaterial {
    Color {
        albedo: [f32; 4],
        metallic: f32,
        roughness: f32,
    },
    PbrFile {
        path: String,
    },
    #[serde(rename = "Grid")]
    GridWireframe,
}

impl SceneMaterial {
    pub fn into_material_shader(self) -> Arc<MaterialShaderDescriptor> {
        match self {
            SceneMaterial::Color {
                albedo,
                metallic,
                roughness,
            } => {
                let a = albedo;
                let rgba = [
                    (a[0].clamp(0.0, 1.0) * 255.0) as u8,
                    (a[1].clamp(0.0, 1.0) * 255.0) as u8,
                    (a[2].clamp(0.0, 1.0) * 255.0) as u8,
                    (a[3].clamp(0.0, 1.0) * 255.0) as u8,
                ];

                let m = (metallic.clamp(0.0, 1.0) * 255.0) as u8;
                let r = (roughness.clamp(0.0, 1.0) * 255.0) as u8;

                let tex_1x1 = || TextureSize {
                    width: 1,
                    height: 1,
                    ..Default::default()
                };

                let tex_1x1_rgba = TextureDescriptor::Data {
                    pixels: rgba.to_vec(),
                    size: tex_1x1(),
                    format: DEFAULT_FORMAT,
                    usages: mat_texture_usages(),
                    texture_dimension: wgpu::TextureDimension::D2,
                    texture_view_dimension: wgpu::TextureViewDimension::D2,
                    filter_mode: FilterMode::default(),
                };

                let tex_1x1_r = TextureDescriptor::Data {
                    pixels: vec![m],
                    size: tex_1x1(),
                    format: wgpu::TextureFormat::R8Unorm,
                    usages: mat_texture_usages(),
                    texture_dimension: wgpu::TextureDimension::D2,
                    texture_view_dimension: wgpu::TextureViewDimension::D2,
                    filter_mode: FilterMode::default(),
                };

                let tex_1x1_r2 = TextureDescriptor::Data {
                    pixels: vec![r],
                    size: tex_1x1(),
                    format: wgpu::TextureFormat::R8Unorm,
                    usages: mat_texture_usages(),
                    texture_dimension: wgpu::TextureDimension::D2,
                    texture_view_dimension: wgpu::TextureViewDimension::D2,
                    filter_mode: FilterMode::default(),
                };

                let normal_tex = TextureDescriptor::Data {
                    pixels: vec![128, 128, 255, 255],
                    size: tex_1x1(),
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usages: mat_texture_usages(),
                    texture_dimension: wgpu::TextureDimension::D2,
                    texture_view_dimension: wgpu::TextureViewDimension::D2,
                    filter_mode: FilterMode::default(),
                };

                let occlusion_tex = TextureDescriptor::Data {
                    pixels: vec![128],
                    size: tex_1x1(),
                    format: wgpu::TextureFormat::R8Unorm,
                    usages: mat_texture_usages(),
                    texture_dimension: wgpu::TextureDimension::D2,
                    texture_view_dimension: wgpu::TextureViewDimension::D2,
                    filter_mode: FilterMode::default(),
                };

                let emissive_tex = TextureDescriptor::Data {
                    pixels: vec![0, 0, 0, 0],
                    size: tex_1x1(),
                    format: DEFAULT_FORMAT,
                    usages: mat_texture_usages(),
                    texture_dimension: wgpu::TextureDimension::D2,
                    texture_view_dimension: wgpu::TextureViewDimension::D2,
                    filter_mode: FilterMode::default(),
                };

                log::info!(
                    "Creating material: albedo=({:.2},{:.2},{:.2}) metallic={:.2} roughness={:.2}",
                    a[0],
                    a[1],
                    a[2],
                    metallic,
                    roughness,
                );

                let pbr = PBRMaterialShaderDescriptor {
                    name: Some("SceneMaterial::Color".into()),
                    normal: normal_tex,
                    albedo: tex_1x1_rgba,
                    // Albedo, metallic and roughness live in the factors, so
                    // the shader can skip those three texture fetches. The
                    // factor is linear, matching the hardware sRGB decode the
                    // 1x1 texture used to receive.
                    albedo_factor: Vector3::new(
                        srgb_to_linear(a[0]),
                        srgb_to_linear(a[1]),
                        srgb_to_linear(a[2]),
                    ),
                    metallic: tex_1x1_r,
                    metallic_factor: metallic,
                    roughness: tex_1x1_r2,
                    roughness_factor: roughness,
                    occlusion: occlusion_tex,
                    emissive: emissive_tex,
                    // An alpha below one in a scene colour means the author
                    // wants a blended surface. Keep opaque colours on the
                    // cheaper opaque path.
                    alpha_mode: if a[3] < 1.0 {
                        AlphaMode::Blend
                    } else {
                        AlphaMode::Opaque
                    },
                    alpha_factor: a[3],
                    // Occlusion keeps its baked 0.5 value to preserve the
                    // authored ambient level; everything else is neutral.
                    texture_flags: MATERIAL_TEXTURE_OCCLUSION,
                    custom_material_shader: None,
                };

                Arc::new(pbr.into())
            }
            SceneMaterial::PbrFile { .. } => {
                // Fallback to default material for now
                Arc::new(MaterialShaderDescriptor::default())
            }
            SceneMaterial::GridWireframe => {
                let mut base = wireframe_descriptor();
                base.vertex_stage_layouts = Some(vec![
                    VertexStageLayout::Custom(VertexBufferLayout {
                        array_stride: 56,
                        step_mode: VertexStepMode::Vertex,
                        attributes: &[VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: VertexFormat::Float32x3,
                        }],
                    }),
                    VertexStageLayout::InstanceData,
                ]);
                Arc::new(base)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ignit_shader_core::VariableType;

    #[test]
    fn colour_materials_use_factors_and_skip_neutral_textures() {
        let material = SceneMaterial::Color {
            albedo: [1.0, 0.0, 0.0, 0.5],
            metallic: 0.75,
            roughness: 0.25,
        }
        .into_material_shader();

        let bytes = match material.variables.last().unwrap() {
            VariableType::Buffer(buffer) => &buffer.data,
            other => panic!("expected a PBR factor buffer, got {other:?}"),
        };

        assert_eq!(f32::from_le_bytes(bytes[0..4].try_into().unwrap()), 1.0);
        assert_eq!(f32::from_le_bytes(bytes[12..16].try_into().unwrap()), 0.75);
        assert_eq!(f32::from_le_bytes(bytes[16..20].try_into().unwrap()), 0.25);
        assert_eq!(f32::from_le_bytes(bytes[20..24].try_into().unwrap()), 0.5);
        assert_eq!(
            u32::from_le_bytes(bytes[28..32].try_into().unwrap()),
            MATERIAL_TEXTURE_OCCLUSION,
        );
    }
}
