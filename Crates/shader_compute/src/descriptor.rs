use std::num::{NonZeroU32, NonZeroU64};
use std::sync::Arc;

use ignit_shader_core::ShaderDescriptor;
use wgpu::{
    BindGroupLayoutEntry, BindingType, BufferBindingType, SamplerBindingType, ShaderStages,
    StorageTextureAccess, TextureFormat, TextureSampleType, TextureViewDimension,
};

/// One binding in a compute shader's bind group.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ComputeBindingLayout {
    Buffer {
        binding: u32,
        ty: BufferBindingType,
        has_dynamic_offset: bool,
        min_binding_size: Option<NonZeroU64>,
        count: Option<NonZeroU32>,
    },
    Texture {
        binding: u32,
        sample_type: TextureSampleType,
        view_dimension: TextureViewDimension,
        multisampled: bool,
        count: Option<NonZeroU32>,
    },
    Sampler {
        binding: u32,
        ty: SamplerBindingType,
        count: Option<NonZeroU32>,
    },
    StorageTexture {
        binding: u32,
        access: StorageTextureAccess,
        format: TextureFormat,
        view_dimension: TextureViewDimension,
        count: Option<NonZeroU32>,
    },
}

impl ComputeBindingLayout {
    /// A uniform buffer binding.
    pub fn uniform_buffer(binding: u32) -> Self {
        Self::Buffer {
            binding,
            ty: BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
            count: None,
        }
    }

    /// A storage buffer binding.
    pub fn storage_buffer(binding: u32, read_only: bool) -> Self {
        Self::Buffer {
            binding,
            ty: BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
            count: None,
        }
    }

    /// A sampled texture binding. A sampler is a separate binding.
    pub fn texture(
        binding: u32,
        sample_type: TextureSampleType,
        view_dimension: TextureViewDimension,
    ) -> Self {
        Self::Texture {
            binding,
            sample_type,
            view_dimension,
            multisampled: false,
            count: None,
        }
    }

    /// A sampler binding.
    pub fn sampler(binding: u32, ty: SamplerBindingType) -> Self {
        Self::Sampler {
            binding,
            ty,
            count: None,
        }
    }

    /// A storage texture binding.
    pub fn storage_texture(
        binding: u32,
        access: StorageTextureAccess,
        format: TextureFormat,
        view_dimension: TextureViewDimension,
    ) -> Self {
        Self::StorageTexture {
            binding,
            access,
            format,
            view_dimension,
            count: None,
        }
    }

    /// Binding index of this layout entry.
    pub fn binding(&self) -> u32 {
        match self {
            Self::Buffer { binding, .. }
            | Self::Texture { binding, .. }
            | Self::Sampler { binding, .. }
            | Self::StorageTexture { binding, .. } => *binding,
        }
    }

    /// Declared array count, if this binding is an array.
    pub fn count(&self) -> Option<NonZeroU32> {
        match self {
            Self::Buffer { count, .. }
            | Self::Texture { count, .. }
            | Self::Sampler { count, .. }
            | Self::StorageTexture { count, .. } => *count,
        }
    }

    pub(crate) fn layout_entry(&self) -> BindGroupLayoutEntry {
        let visibility = ShaderStages::COMPUTE;

        match self {
            Self::Buffer {
                binding,
                ty,
                has_dynamic_offset,
                min_binding_size,
                count,
            } => BindGroupLayoutEntry {
                binding: *binding,
                visibility,
                ty: BindingType::Buffer {
                    ty: *ty,
                    has_dynamic_offset: *has_dynamic_offset,
                    min_binding_size: *min_binding_size,
                },
                count: *count,
            },
            Self::Texture {
                binding,
                sample_type,
                view_dimension,
                multisampled,
                count,
            } => BindGroupLayoutEntry {
                binding: *binding,
                visibility,
                ty: BindingType::Texture {
                    sample_type: *sample_type,
                    view_dimension: *view_dimension,
                    multisampled: *multisampled,
                },
                count: *count,
            },
            Self::Sampler { binding, ty, count } => BindGroupLayoutEntry {
                binding: *binding,
                visibility,
                ty: BindingType::Sampler(*ty),
                count: *count,
            },
            Self::StorageTexture {
                binding,
                access,
                format,
                view_dimension,
                count,
            } => BindGroupLayoutEntry {
                binding: *binding,
                visibility,
                ty: BindingType::StorageTexture {
                    access: *access,
                    format: *format,
                    view_dimension: *view_dimension,
                },
                count: *count,
            },
        }
    }
}

/// Describes a compute shader assembled from named shader nodes and raw WGSL.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComputeShaderDescriptor {
    pub name: Option<String>,
    /// Root node names resolved against the registry.
    pub nodes: &'static [&'static str],
    /// Raw WGSL appended after the nodes, usually containing the entrypoint.
    pub raw_source: Option<Arc<str>>,
    pub entrypoint: &'static str,
    /// Optional convenience for dispatch helpers. Must match the WGSL
    /// `@workgroup_size`.
    pub workgroup_size: Option<[u32; 3]>,
    /// Bind-group entries for the shader's own resources.
    ///
    /// When a world bind group is supplied to [`ComputeShaderProgram`], this
    /// group is bound at index 1. Otherwise it is bound at index 0.
    ///
    /// [`ComputeShaderProgram`]: crate::ComputeShaderProgram
    pub bindings: Vec<ComputeBindingLayout>,
}

impl ShaderDescriptor for ComputeShaderDescriptor {
    fn name(&self) -> Option<String> {
        self.name.clone()
    }

    fn nodes(&self) -> &[&str] {
        self.nodes
    }

    fn raw_source(&self) -> Option<Arc<str>> {
        self.raw_source.clone()
    }

    fn stages(&self) -> ShaderStages {
        ShaderStages::COMPUTE
    }

    fn bind_group_layout_entries(&self) -> Vec<BindGroupLayoutEntry> {
        self.bindings
            .iter()
            .map(ComputeBindingLayout::layout_entry)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_uses_compute_stage_and_bindings() {
        let descriptor = ComputeShaderDescriptor {
            name: Some("test".into()),
            nodes: &[],
            raw_source: Some(Arc::from("@compute @workgroup_size(1) fn main() {}")),
            entrypoint: "main",
            workgroup_size: Some([1, 1, 1]),
            bindings: vec![
                ComputeBindingLayout::storage_buffer(0, true),
                ComputeBindingLayout::storage_buffer(1, false),
            ],
        };

        let entries = descriptor.bind_group_layout_entries();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].visibility, ShaderStages::COMPUTE);
        assert_eq!(
            entries[1].ty,
            BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            }
        );
    }

    #[test]
    fn depth_texture_does_not_require_a_sampler_binding() {
        let layout =
            ComputeBindingLayout::texture(3, TextureSampleType::Depth, TextureViewDimension::D2);

        let entry = layout.layout_entry();

        assert_eq!(entry.binding, 3);
        assert!(matches!(entry.ty, BindingType::Texture { .. }));
    }

    #[test]
    fn storage_texture_layout_is_preserved() {
        let layout = ComputeBindingLayout::storage_texture(
            0,
            StorageTextureAccess::WriteOnly,
            TextureFormat::Rgba16Float,
            TextureViewDimension::D3,
        );

        let entry = layout.layout_entry();

        assert_eq!(
            entry.ty,
            BindingType::StorageTexture {
                access: StorageTextureAccess::WriteOnly,
                format: TextureFormat::Rgba16Float,
                view_dimension: TextureViewDimension::D3,
            }
        );
    }
}
