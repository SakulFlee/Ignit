//! Minimal scissored UI renderer for the spike.

use bytemuck::{Pod, Zeroable};
use ignit::wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BlendState, Buffer, BufferBindingType,
    BufferDescriptor, BufferUsages, ColorTargetState, ColorWrites, CommandEncoder, Device,
    Extent3d, FragmentState, LoadOp, MultisampleState, Operations, Origin3d,
    PipelineLayoutDescriptor, PrimitiveState, PrimitiveTopology, Queue, RenderPassColorAttachment,
    RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor, SamplerBindingType,
    SamplerDescriptor, ShaderModuleDescriptor, ShaderSource, ShaderStages, StoreOp,
    TexelCopyBufferLayout, TexelCopyTextureInfo, Texture, TextureAspect, TextureDescriptor,
    TextureDimension, TextureFormat, TextureSampleType, TextureView, TextureViewDescriptor,
    TextureViewDimension, VertexAttribute, VertexBufferLayout, VertexFormat, VertexState,
    VertexStepMode,
};

use crate::draw::{DrawList, DrawVertex};
use crate::layout::Rect;
use crate::text::GlyphAtlas;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ScreenUniform {
    size: [f32; 2],
    _pad: [f32; 2],
}

const SHADER: &str = r#"
struct Screen {
    size: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> screen: Screen;

@group(1) @binding(0) var atlas_tex: texture_2d<f32>;
@group(1) @binding(1) var atlas_sampler: sampler;
@group(1) @binding(2) var image_tex: texture_2d<f32>;
@group(1) @binding(3) var image_sampler: sampler;

struct VertexIn {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) params: vec4<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) params: vec4<f32>,
};

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    var output: VertexOut;
    let ndc = vec2<f32>(
        input.position.x / screen.size.x * 2.0 - 1.0,
        1.0 - input.position.y / screen.size.y * 2.0,
    );
    output.position = vec4<f32>(ndc, 0.0, 1.0);
    output.color = input.color;
    output.uv = input.uv;
    output.params = input.params;
    return output;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    var color = input.color;

    // Sample both textures before the branch so uniformity analysis stays
    // happy on every backend.
    let atlas_alpha = textureSample(atlas_tex, atlas_sampler, input.uv).r;
    let image_sample = textureSample(image_tex, image_sampler, input.uv);
    let mode = u32(input.params.y + 0.5);

    if (mode == 0u) {
        let size = max(input.params.zw, vec2<f32>(1.0));
        let half = size * 0.5;
        let p = (input.uv - vec2<f32>(0.5)) * size;
        let radius = min(input.params.x, min(half.x, half.y));
        let q = abs(p) - (half - vec2<f32>(radius));
        let distance = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
        let aa = max(fwidth(distance), 0.0001);
        color.a *= 1.0 - smoothstep(-aa, aa, distance);
    } else if (mode == 1u) {
        color.a *= atlas_alpha;
    } else if (mode == 2u) {
        color *= image_sample;
    }

    return color;
}
"#;

pub struct GpuUiRenderer {
    pipeline: RenderPipeline,
    screen_buffer: Buffer,
    screen_bind_group: BindGroup,
    texture_bind_group: BindGroup,
    atlas_texture: Texture,
    #[allow(dead_code)]
    image_texture: Texture,
    vertex_buffer: Buffer,
    vertex_capacity: u64,
    atlas_size: u32,
}

impl GpuUiRenderer {
    pub fn new(device: &Device, queue: &Queue, format: TextureFormat) -> Self {
        let screen_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("ui-spike screen layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let texture_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("ui-spike texture layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let screen_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("ui-spike screen uniform"),
            size: std::mem::size_of::<ScreenUniform>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let screen_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("ui-spike screen bind group"),
            layout: &screen_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: screen_buffer.as_entire_binding(),
            }],
        });

        let atlas_size = 1024;
        let atlas_texture = make_texture(
            device,
            "ui-spike alpha atlas",
            atlas_size,
            TextureFormat::R8Unorm,
        );
        let image_texture =
            make_texture(device, "ui-spike checker", 8, TextureFormat::Rgba8UnormSrgb);
        let checker = checkerboard();
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &image_texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &checker,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(8 * 4),
                rows_per_image: Some(8),
            },
            Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
        );

        let atlas_view = atlas_texture.create_view(&TextureViewDescriptor::default());
        let image_view = image_texture.create_view(&TextureViewDescriptor::default());
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("ui-spike sampler"),
            mag_filter: ignit::wgpu::FilterMode::Linear,
            min_filter: ignit::wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let texture_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("ui-spike texture bind group"),
            layout: &texture_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&atlas_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&sampler),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::TextureView(&image_view),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: BindingResource::Sampler(&sampler),
                },
            ],
        });

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("ui-spike shader"),
            source: ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("ui-spike pipeline layout"),
            bind_group_layouts: &[Some(&screen_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("ui-spike pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(VertexBufferLayout {
                    array_stride: std::mem::size_of::<DrawVertex>() as u64,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        VertexAttribute {
                            format: VertexFormat::Float32x4,
                            offset: 8,
                            shader_location: 1,
                        },
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 24,
                            shader_location: 2,
                        },
                        VertexAttribute {
                            format: VertexFormat::Float32x4,
                            offset: 32,
                            shader_location: 3,
                        },
                    ],
                })],
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: ignit::wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: ignit::wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(ColorTargetState {
                    format,
                    blend: Some(BlendState {
                        color: ignit::wgpu::BlendComponent {
                            src_factor: ignit::wgpu::BlendFactor::SrcAlpha,
                            dst_factor: ignit::wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: ignit::wgpu::BlendOperation::Add,
                        },
                        alpha: ignit::wgpu::BlendComponent {
                            src_factor: ignit::wgpu::BlendFactor::One,
                            dst_factor: ignit::wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: ignit::wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let vertex_capacity = 256 * 1024;
        let vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("ui-spike vertices"),
            size: vertex_capacity,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            screen_buffer,
            screen_bind_group,
            texture_bind_group,
            atlas_texture,
            image_texture,
            vertex_buffer,
            vertex_capacity,
            atlas_size,
        }
    }

    pub fn upload_atlas(&self, queue: &Queue, atlas: &mut GlyphAtlas) {
        if !atlas.dirty {
            return;
        }
        if atlas.size != self.atlas_size {
            return;
        }
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &self.atlas_texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &atlas.pixels,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas.size),
                rows_per_image: Some(atlas.size),
            },
            Extent3d {
                width: atlas.size,
                height: atlas.size,
                depth_or_array_layers: 1,
            },
        );
        atlas.dirty = false;
    }

    pub fn render(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        target: &TextureView,
        draw: &DrawList,
        screen: (f32, f32),
    ) {
        if draw.vertices.is_empty() {
            return;
        }

        let uniform = ScreenUniform {
            size: [screen.0.max(1.0), screen.1.max(1.0)],
            _pad: [0.0; 2],
        };
        queue.write_buffer(&self.screen_buffer, 0, bytemuck::bytes_of(&uniform));

        let bytes = bytemuck::cast_slice(&draw.vertices);
        if bytes.len() as u64 > self.vertex_capacity {
            self.vertex_capacity = (bytes.len() as u64).next_power_of_two();
            self.vertex_buffer = device.create_buffer(&BufferDescriptor {
                label: Some("ui-spike vertices"),
                size: self.vertex_capacity,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        queue.write_buffer(&self.vertex_buffer, 0, bytes);

        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("ui-spike pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.screen_bind_group, &[]);
        pass.set_bind_group(1, &self.texture_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        let bounds = Rect {
            x: 0.0,
            y: 0.0,
            w: screen.0,
            h: screen.1,
        };
        for batch in &draw.batches {
            if batch.count == 0 {
                continue;
            }
            let Some(clip) = batch.clip.intersect(bounds) else {
                continue;
            };
            pass.set_scissor_rect(
                clip.x.floor().max(0.0) as u32,
                clip.y.floor().max(0.0) as u32,
                clip.w.ceil().max(1.0) as u32,
                clip.h.ceil().max(1.0) as u32,
            );
            pass.draw(batch.start..batch.start + batch.count, 0..1);
        }
    }
}

fn make_texture(device: &Device, label: &str, size: u32, format: TextureFormat) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some(label),
        size: Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage: ignit::wgpu::TextureUsages::TEXTURE_BINDING | ignit::wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn checkerboard() -> [u8; 8 * 8 * 4] {
    let mut data = [0u8; 8 * 8 * 4];
    for y in 0..8 {
        for x in 0..8 {
            let index = (y * 8 + x) * 4;
            let light = (x / 2 + y / 2) % 2 == 0;
            let value = if light { 220 } else { 70 };
            data[index] = value;
            data[index + 1] = (value as f32 * 0.8) as u8;
            data[index + 2] = (value as f32 * 0.6) as u8;
            data[index + 3] = 255;
        }
    }
    data
}
