//! Compute skinning for skinned models.

use ignit_model::Model;
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BufferBindingType, CommandEncoder,
    ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor, Device,
    PipelineLayoutDescriptor, ShaderModuleDescriptor, ShaderSource, ShaderStages,
};

const SKIN_COMPUTE_WGSL: &str = include_str!("skin_compute.wgsl");
const WORKGROUP_SIZE: u32 = 64;

/// Shared compute pipeline that skins vertices on the GPU.
pub struct ComputeSkinning {
    pipeline: ComputePipeline,
    layout: BindGroupLayout,
}

impl ComputeSkinning {
    pub fn new(device: &Device) -> Self {
        let module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Ignit::SkinComputeShader"),
            source: ShaderSource::Wgsl(SKIN_COMPUTE_WGSL.into()),
        });

        let mut entries = Vec::new();
        for binding in 0..4 {
            entries.push(BindGroupLayoutEntry {
                binding,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            });
        }
        entries.push(BindGroupLayoutEntry {
            binding: 4,
            visibility: ShaderStages::COMPUTE,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });

        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Ignit::SkinComputeLayout"),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Ignit::SkinComputePipelineLayout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("Ignit::SkinComputePipeline"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self { pipeline, layout }
    }

    /// Dispatches skinning for one model, when all compute buffers exist.
    pub fn dispatch(&self, encoder: &mut CommandEncoder, device: &Device, model: &Model) {
        let (Some(rest), Some(joints), Some(weights), Some(matrices), Some(output)) = (
            model.skin_rest_buffer(),
            model.skin_joint_buffer(),
            model.skin_weight_buffer(),
            model.skin_matrix_buffer(),
            model.compute_skinned_vertex_buffer(),
        ) else {
            return;
        };

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Ignit::SkinComputeBindGroup"),
            layout: &self.layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::Buffer(rest.as_entire_buffer_binding()),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Buffer(joints.as_entire_buffer_binding()),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::Buffer(weights.as_entire_buffer_binding()),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: BindingResource::Buffer(matrices.as_entire_buffer_binding()),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: BindingResource::Buffer(output.as_entire_buffer_binding()),
                },
            ],
        });

        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("Ignit::SkinCompute"),
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        let workgroups = model.skinned_vertex_count().div_ceil(WORKGROUP_SIZE).max(1);
        pass.dispatch_workgroups(workgroups, 1, 1);
    }
}
