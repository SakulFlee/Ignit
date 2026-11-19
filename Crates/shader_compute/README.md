# ignit_shader_compute

Compute shader assembly, binding layouts, and pipeline creation for Ignit.

Part of the [Ignit](https://crates.io/crates/ignit) engine & framework.

[Ignit repository](https://forgejo.sakul-flee.de/SakulFlee/Ignit) | [GitHub mirror](https://github.com/SakulFlee/Ignit)

Licensed under [MIT OR Apache-2.0](https://forgejo.sakul-flee.de/SakulFlee/Ignit/src/branch/main/LICENSE).

## Overview

`ComputeShaderDescriptor` assembles WGSL from named shader nodes plus raw
source, then declares explicit compute bindings. `ComputeShader` creates the
pipeline layout, bind group, and cached pipeline.

Bindings use `ComputeBindingLayout`, which supports:

- uniform, read-only storage, and read/write storage buffers,
- sampled textures without an implicit sampler,
- standalone samplers,
- storage textures.

Binding arrays (a layout `count` greater than one) are not supported by the
binding builder yet; the descriptor rejects them when a shader is created.

Resources are supplied with `ComputeBindings`, so externally owned buffers and
render targets can be bound without creating descriptor-owned data.

## Basic use

```rust
use std::sync::Arc;
use ignit_shader_compute::{
    ComputeBindingLayout, ComputeBindings, ComputeShader, ComputeShaderDescriptor,
};

let descriptor = ComputeShaderDescriptor {
    name: Some("Particle Update".into()),
    nodes: &[],
    raw_source: Some(Arc::from(
        r#"
struct Particle {
    position: vec4<f32>,
    velocity: vec4<f32>,
}

@group(0) @binding(0) var<storage, read> input: array<Particle>;
@group(0) @binding(1) var<storage, read_write> output: array<Particle>;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    var particle = input[id.x];
    particle.position = particle.position + particle.velocity;
    output[id.x] = particle;
}
"#,
    )),
    entrypoint: "main",
    workgroup_size: Some([64, 1, 1]),
    bindings: vec![
        ComputeBindingLayout::storage_buffer(0, true),
        ComputeBindingLayout::storage_buffer(1, false),
    ],
};

let shader = ComputeShader::from_descriptor(
    &descriptor,
    ComputeBindings::new()
        .buffer(0, &input_buffer)
        .buffer(1, &output_buffer),
    None,
    &device,
)?;

let mut encoder = device.create_command_encoder(&Default::default());
shader.record(&mut encoder, None, [workgroups, 1, 1]);
queue.submit([encoder.finish()]);
```

When a world bind group layout is passed to `ComputeShader::from_descriptor`,
the engine world group is group 0 and the shader's own bindings are group 1.
Otherwise the shader's bindings are group 0.

## Runtime workloads

Register a workload through the ECS resource so it runs before the scene's
shadow pass:

```rust
use std::sync::Arc;
use ignit_app::{ComputeWorkload, ComputeWorkloadContext, ComputeWorkloads};

struct Particles {
    shader: ignit_shader_compute::ComputeShader,
}

impl ComputeWorkload for Particles {
    fn name(&self) -> &str {
        "particles"
    }

    fn record(&self, ctx: &mut ComputeWorkloadContext<'_>) {
        self.shader.record(ctx.encoder, None, [64, 1, 1]);
    }
}

ecs.insert_resource(ComputeWorkloads::new());
ecs.get_resource::<ComputeWorkloads>()
    .expect("compute workloads")
    .add(Arc::new(Particles { shader }));
```
