use std::sync::Arc;

use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupLayout, CommandEncoder, ComputePass,
    ComputePassDescriptor, ComputePipeline, Device,
};

use crate::{
    ComputeBindings, ComputePipelineCache, ComputeShaderDescriptor, ComputeShaderError,
    ComputeShaderProgram,
};

/// A realized compute shader: a compiled program, pipeline, and bind group.
#[derive(Debug)]
pub struct ComputeShader {
    program: Arc<ComputeShaderProgram>,
    pipeline: Arc<ComputePipeline>,
    bind_group: Option<BindGroup>,
    bind_group_index: u32,
    bindings: ComputeBindings,
}

impl ComputeShader {
    /// Creates a compute shader without a shared pipeline cache.
    pub fn from_descriptor(
        descriptor: &ComputeShaderDescriptor,
        bindings: ComputeBindings,
        world_bind_group_layout: Option<&BindGroupLayout>,
        device: &Device,
    ) -> Result<Self, ComputeShaderError> {
        let mut pipeline_cache = ComputePipelineCache::default();
        Self::from_descriptor_with_cache(
            descriptor,
            bindings,
            world_bind_group_layout,
            device,
            &mut pipeline_cache,
        )
    }

    /// Creates a compute shader using a shared pipeline cache.
    pub fn from_descriptor_with_cache(
        descriptor: &ComputeShaderDescriptor,
        bindings: ComputeBindings,
        world_bind_group_layout: Option<&BindGroupLayout>,
        device: &Device,
        pipeline_cache: &mut ComputePipelineCache,
    ) -> Result<Self, ComputeShaderError> {
        let program = Arc::new(ComputeShaderProgram::from_descriptor(
            descriptor,
            world_bind_group_layout,
            device,
        )?);

        Self::from_program(program, bindings, device, pipeline_cache)
    }

    /// Creates a compute shader from an existing program.
    ///
    /// Reusing the program lets the pipeline cache reuse the compiled
    /// pipeline while a caller creates a new bind group for changed resources.
    pub fn from_program(
        program: Arc<ComputeShaderProgram>,
        bindings: ComputeBindings,
        device: &Device,
        pipeline_cache: &mut ComputePipelineCache,
    ) -> Result<Self, ComputeShaderError> {
        validate_bindings(&program, &bindings)?;

        let bind_group = program.bind_group_layout().map(|layout| {
            device.create_bind_group(&BindGroupDescriptor {
                label: program.name(),
                layout,
                entries: &bindings.bind_group_entries(),
            })
        });
        let pipeline = pipeline_cache.pipeline(&program, device);

        Ok(Self {
            bind_group_index: program.bind_group_index(),
            program,
            pipeline,
            bind_group,
            bindings,
        })
    }

    pub fn pipeline(&self) -> &ComputePipeline {
        &self.pipeline
    }

    pub fn bind_group(&self) -> Option<&BindGroup> {
        self.bind_group.as_ref()
    }

    /// Bind-group index of this shader's own resources.
    ///
    /// This is 0 unless the program was built with a world bind group, in
    /// which case the own resources are at 1.
    pub fn bind_group_index(&self) -> u32 {
        self.bind_group_index
    }

    pub fn program(&self) -> &ComputeShaderProgram {
        &self.program
    }

    pub fn program_arc(&self) -> &Arc<ComputeShaderProgram> {
        &self.program
    }

    pub fn bindings(&self) -> &ComputeBindings {
        &self.bindings
    }

    pub fn workgroup_size(&self) -> Option<[u32; 3]> {
        self.program.workgroup_size()
    }

    /// Binds and dispatches this shader into an existing compute pass.
    ///
    /// The caller is responsible for binding the world bind group at group 0
    /// when the program uses one.
    pub fn dispatch(&self, pass: &mut ComputePass<'_>, groups: [u32; 3]) {
        pass.set_pipeline(&self.pipeline);
        if let Some(bind_group) = self.bind_group.as_ref() {
            pass.set_bind_group(self.bind_group_index, bind_group, &[]);
        }
        pass.dispatch_workgroups(groups[0], groups[1], groups[2]);
    }

    /// Records a compute pass containing a single dispatch.
    ///
    /// `world_bind_group` is only used when the program was built with a world
    /// bind group layout.
    pub fn record(
        &self,
        encoder: &mut CommandEncoder,
        world_bind_group: Option<&BindGroup>,
        groups: [u32; 3],
    ) {
        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: self.program.name(),
            ..Default::default()
        });

        if self.bind_group_index == 1
            && let Some(world_bind_group) = world_bind_group
        {
            pass.set_bind_group(0, world_bind_group, &[]);
        }

        self.dispatch(&mut pass, groups);
    }
}

fn validate_bindings(
    program: &ComputeShaderProgram,
    bindings: &ComputeBindings,
) -> Result<(), ComputeShaderError> {
    if program.bindings().is_empty() {
        if !bindings.is_empty() {
            return Err(ComputeShaderError::BindingsWithoutLayout);
        }
        return Ok(());
    }

    for layout in program.bindings() {
        if layout.count().is_some() {
            return Err(ComputeShaderError::UnsupportedBindingArray {
                binding: layout.binding(),
            });
        }

        if !bindings.has_binding(layout.binding()) {
            return Err(ComputeShaderError::MissingBinding {
                binding: layout.binding(),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ComputeBindingLayout, ComputeShaderDescriptor};

    fn storage_descriptor() -> ComputeShaderDescriptor {
        ComputeShaderDescriptor {
            name: Some("Test Compute".into()),
            nodes: &[],
            raw_source: Some(Arc::from(
                r#"
struct Particle {
    position: vec4<f32>,
    velocity: vec4<f32>,
}

@group(0) @binding(0) var<storage, read> input: array<Particle>;
@group(0) @binding(1) var<storage, read_write> output: array<Particle>;

@compute @workgroup_size(2)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    var particle = input[id.x];
    particle.position = particle.position + particle.velocity;
    output[id.x] = particle;
}
"#,
            )),
            entrypoint: "main",
            workgroup_size: Some([2, 1, 1]),
            bindings: vec![
                ComputeBindingLayout::storage_buffer(0, true),
                ComputeBindingLayout::storage_buffer(1, false),
            ],
        }
    }

    fn storage_buffers(device: &Device) -> (wgpu::Buffer, wgpu::Buffer) {
        let input = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Test Compute Input"),
            size: 64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Test Compute Output"),
            size: 64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        (input, output)
    }

    #[test]
    fn compute_shader_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ComputeShader>();
    }

    #[test]
    fn creates_and_dispatches_on_noop_backend() {
        let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let (input, output) = storage_buffers(&device);
        let bindings = ComputeBindings::new().buffer(0, &input).buffer(1, &output);

        let shader = ComputeShader::from_descriptor(&storage_descriptor(), bindings, None, &device)
            .expect("compute shader");

        let mut encoder = device.create_command_encoder(&Default::default());
        shader.record(&mut encoder, None, [1, 1, 1]);
        drop(encoder.finish());
    }

    #[test]
    fn missing_binding_is_rejected() {
        let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let (input, _output) = storage_buffers(&device);

        let error = ComputeShader::from_descriptor(
            &storage_descriptor(),
            ComputeBindings::new().buffer(0, &input),
            None,
            &device,
        )
        .expect_err("missing output binding");

        assert!(matches!(
            error,
            ComputeShaderError::MissingBinding { binding: 1 }
        ));
    }

    #[test]
    fn resources_without_a_layout_are_rejected() {
        let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let (input, _output) = storage_buffers(&device);
        let mut descriptor = storage_descriptor();
        descriptor.bindings.clear();

        let error = ComputeShader::from_descriptor(
            &descriptor,
            ComputeBindings::new().buffer(0, &input),
            None,
            &device,
        )
        .expect_err("resources without layout");

        assert!(matches!(error, ComputeShaderError::BindingsWithoutLayout));
    }

    #[test]
    fn pipeline_cache_reuses_a_shared_program() {
        let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let (input, output) = storage_buffers(&device);
        let descriptor = storage_descriptor();
        let program = Arc::new(
            ComputeShaderProgram::from_descriptor(&descriptor, None, &device)
                .expect("compute program"),
        );
        let mut cache = ComputePipelineCache::default();

        let first = ComputeShader::from_program(
            Arc::clone(&program),
            ComputeBindings::new().buffer(0, &input).buffer(1, &output),
            &device,
            &mut cache,
        )
        .expect("first shader");
        let second = ComputeShader::from_program(
            Arc::clone(&program),
            ComputeBindings::new().buffer(0, &input).buffer(1, &output),
            &device,
            &mut cache,
        )
        .expect("second shader");

        assert!(std::ptr::eq(first.pipeline(), second.pipeline()));
        assert_eq!(cache.pipeline_count(), 1);
    }
}
