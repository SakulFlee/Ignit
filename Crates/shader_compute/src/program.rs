use std::sync::atomic::{AtomicU64, Ordering};

use wgpu::{
    BindGroupLayout, ComputePipeline, ComputePipelineDescriptor, Device, PipelineLayout,
    PipelineLayoutDescriptor, ShaderModule,
};

use ignit_shader_core::ShaderDescriptor;

use crate::{ComputeBindingLayout, ComputeShaderDescriptor, ComputeShaderError};

static NEXT_PROGRAM_ID: AtomicU64 = AtomicU64::new(1);

/// A compiled compute shader program shared by one or more bind groups.
#[derive(Debug)]
pub struct ComputeShaderProgram {
    id: u64,
    name: Option<String>,
    module: ShaderModule,
    bind_group_layout: Option<BindGroupLayout>,
    pipeline_layout: PipelineLayout,
    entrypoint: &'static str,
    bind_group_index: u32,
    workgroup_size: Option<[u32; 3]>,
    bindings: Vec<ComputeBindingLayout>,
}

impl ComputeShaderProgram {
    /// Builds a program from a descriptor.
    ///
    /// When `world_bind_group_layout` is supplied, group 0 is the engine world
    /// group and the descriptor's bindings are group 1. Otherwise the
    /// descriptor's bindings are group 0.
    pub fn from_descriptor(
        descriptor: &ComputeShaderDescriptor,
        world_bind_group_layout: Option<&BindGroupLayout>,
        device: &Device,
    ) -> Result<Self, ComputeShaderError> {
        let module = descriptor.shader_module(device)?;
        let bind_group_layout = descriptor.bind_group_layout_only(device);

        let mut bind_group_layouts = Vec::new();
        if let Some(world_layout) = world_bind_group_layout {
            bind_group_layouts.push(Some(world_layout));
        }
        if let Some(layout) = bind_group_layout.as_ref() {
            bind_group_layouts.push(Some(layout));
        }

        let bind_group_index = u32::from(world_bind_group_layout.is_some());
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: descriptor.name.as_deref(),
            bind_group_layouts: &bind_group_layouts,
            immediate_size: 0,
        });

        Ok(Self {
            id: NEXT_PROGRAM_ID.fetch_add(1, Ordering::Relaxed),
            name: descriptor.name.clone(),
            module,
            bind_group_layout,
            pipeline_layout,
            entrypoint: descriptor.entrypoint,
            bind_group_index,
            workgroup_size: descriptor.workgroup_size,
            bindings: descriptor.bindings.clone(),
        })
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn bind_group_layout(&self) -> Option<&BindGroupLayout> {
        self.bind_group_layout.as_ref()
    }

    pub fn bind_group_index(&self) -> u32 {
        self.bind_group_index
    }

    pub fn pipeline_layout(&self) -> &PipelineLayout {
        &self.pipeline_layout
    }

    pub fn entrypoint(&self) -> &'static str {
        self.entrypoint
    }

    pub fn workgroup_size(&self) -> Option<[u32; 3]> {
        self.workgroup_size
    }

    pub fn bindings(&self) -> &[ComputeBindingLayout] {
        &self.bindings
    }

    pub(crate) fn create_pipeline(&self, device: &Device) -> ComputePipeline {
        device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: self.name.as_deref(),
            layout: Some(&self.pipeline_layout),
            module: &self.module,
            entry_point: Some(self.entrypoint),
            compilation_options: Default::default(),
            cache: None,
        })
    }
}
