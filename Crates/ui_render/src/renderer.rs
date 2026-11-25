//! Batched wgpu renderer for `ignit_ui::UiDrawList`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bytemuck::{Pod, Zeroable};
use ignit_app::{LayerRenderer, OverlaySubmission, RenderLayer, RenderOverlayContext};
use ignit_ecs::{Entity, World};
use ignit_ui::{
    UiDrawCommand, UiDrawList, UiNode, UiPoint, UiRect, UiResolvedTextStyle, UiTextStyle, UiView,
};
use ignit_ui_text::{GlyphAtlas, TextEngine};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BlendState, Buffer, BufferBindingType,
    BufferDescriptor, BufferUsages, ColorTargetState, ColorWrites, CommandEncoder, Device,
    Extent3d, FragmentState, LoadOp, MultisampleState, Operations, Origin3d,
    PipelineLayoutDescriptor, PrimitiveState, PrimitiveTopology, Queue, RenderPassColorAttachment,
    RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor, Sampler, SamplerBindingType,
    SamplerDescriptor, ShaderModuleDescriptor, ShaderSource, ShaderStages, StoreOp,
    TexelCopyBufferLayout, TexelCopyTextureInfo, Texture, TextureAspect, TextureDescriptor,
    TextureDimension, TextureFormat, TextureSampleType, TextureUsages, TextureView,
    TextureViewDescriptor, TextureViewDimension, VertexAttribute, VertexBufferLayout, VertexFormat,
    VertexState, VertexStepMode,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct UiVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub uv: [f32; 2],
    pub params: [f32; 4],
    pub aux: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UiTextureRef {
    White,
    Atlas(usize),
    Image(u64),
}

#[derive(Clone, Copy, Debug)]
pub struct UiBatch {
    pub start: u32,
    pub count: u32,
    pub clip: UiRect,
    pub mode: u32,
    pub texture: UiTextureRef,
}

#[derive(Clone, Debug, Default)]
pub struct UiRenderStats {
    pub vertices: usize,
    pub batches: usize,
    pub atlas_pages: usize,
}

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
@group(1) @binding(0) var ui_texture: texture_2d<f32>;
@group(1) @binding(1) var ui_sampler: sampler;

struct VertexIn {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) params: vec4<f32>,
    @location(4) aux: vec2<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) params: vec4<f32>,
    @location(3) aux: vec2<f32>,
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
    output.aux = input.aux;
    return output;
}

fn rounded_distance(uv: vec2<f32>, params: vec4<f32>) -> f32 {
    let size = max(params.zw, vec2<f32>(1.0));
    let half = size * 0.5;
    let p = (uv - vec2<f32>(0.5)) * size;
    let radius = min(params.x, min(half.x, half.y));
    let q = abs(p) - (half - vec2<f32>(radius));
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    var color = input.color;
    let sampled = textureSample(ui_texture, ui_sampler, input.uv);
    let mode = u32(input.params.y + 0.5);
    let distance = rounded_distance(input.uv, input.params);
    let aa = max(fwidth(distance), 0.0001);

    if (mode == 0u) {
        color.a *= 1.0 - smoothstep(-aa, aa, distance);
    } else if (mode == 1u) {
        color.a *= sampled.r;
    } else if (mode == 2u) {
        color *= sampled;
    } else if (mode == 3u) {
        let width = max(input.aux.x, 0.5);
        let outer = 1.0 - smoothstep(-aa, aa, distance);
        let inner = 1.0 - smoothstep(-aa, aa, distance + width);
        color.a *= max(outer - inner, 0.0);
    } else if (mode == 4u) {
        let blur = max(input.aux.x, 0.5);
        color.a *= 1.0 - smoothstep(-blur, blur, distance);
    }

    return color;
}
"#;

pub struct UiRenderer {
    pipeline: RenderPipeline,
    screen_buffer: Buffer,
    screen_bind_group: BindGroup,
    texture_layout: BindGroupLayout,
    sampler: Sampler,
    atlas_sampler: Sampler,
    page_textures: Vec<Texture>,
    page_bind_groups: Vec<BindGroup>,
    white_bind_group: BindGroup,
    image_bind_groups: HashMap<u64, BindGroup>,
    image_names: HashMap<String, u64>,
    next_image: u64,
    vertex_buffer: Buffer,
    vertex_capacity: u64,
    pub stats: UiRenderStats,
    pub root_offsets: HashMap<Entity, UiPoint>,
}

impl UiRenderer {
    pub fn new(device: &Device, queue: &Queue, format: TextureFormat) -> Self {
        Self::new_with_atlas_size(device, queue, format, 1024)
    }

    pub fn new_with_atlas_size(
        device: &Device,
        queue: &Queue,
        format: TextureFormat,
        atlas_page_size: u32,
    ) -> Self {
        let screen_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("ignit ui screen layout"),
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
            label: Some("ignit ui texture layout"),
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
            ],
        });

        let screen_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("ignit ui screen uniform"),
            size: std::mem::size_of::<ScreenUniform>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let screen_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("ignit ui screen bind group"),
            layout: &screen_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: screen_buffer.as_entire_binding(),
            }],
        });

        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("ignit ui sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let atlas_sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("ignit ui atlas sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let atlas_texture = make_texture(
            device,
            "ignit ui atlas page",
            atlas_page_size,
            TextureFormat::R8Unorm,
            TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        );
        let white_texture = make_texture(
            device,
            "ignit ui white",
            1,
            TextureFormat::Rgba8UnormSrgb,
            TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        );
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &white_texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &[255u8; 4],
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let white_view = white_texture.create_view(&TextureViewDescriptor::default());
        let white_bind_group = make_texture_bind_group(
            device,
            &texture_layout,
            &white_view,
            &sampler,
            "ignit ui white bind group",
        );
        let atlas_view = atlas_texture.create_view(&TextureViewDescriptor::default());
        let atlas_bind_group = make_texture_bind_group(
            device,
            &texture_layout,
            &atlas_view,
            &atlas_sampler,
            "ignit ui atlas bind group",
        );

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("ignit ui shader"),
            source: ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("ignit ui pipeline layout"),
            bind_group_layouts: &[Some(&screen_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("ignit ui pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(VertexBufferLayout {
                    array_stride: std::mem::size_of::<UiVertex>() as u64,
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
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 48,
                            shader_location: 4,
                        },
                    ],
                })],
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
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
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
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
            label: Some("ignit ui vertices"),
            size: vertex_capacity,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            screen_buffer,
            screen_bind_group,
            texture_layout,
            sampler,
            atlas_sampler,
            page_textures: vec![atlas_texture],
            page_bind_groups: vec![atlas_bind_group],
            white_bind_group,
            image_bind_groups: HashMap::new(),
            image_names: HashMap::new(),
            next_image: 1,
            vertex_buffer,
            vertex_capacity,
            stats: UiRenderStats::default(),
            root_offsets: HashMap::new(),
        }
    }

    pub fn register_texture(
        &mut self,
        device: &Device,
        name: impl Into<String>,
        view: TextureView,
    ) -> u64 {
        let id = self.next_image;
        self.next_image += 1;
        let bind_group = make_texture_bind_group(
            device,
            &self.texture_layout,
            &view,
            &self.sampler,
            "ignit ui image bind group",
        );
        self.image_bind_groups.insert(id, bind_group);
        self.image_names.insert(name.into(), id);
        id
    }

    pub fn unregister_texture(&mut self, id: u64) {
        self.image_bind_groups.remove(&id);
        self.image_names.retain(|_, candidate| *candidate != id);
    }

    pub fn set_root_offset(&mut self, root: Entity, offset: UiPoint) {
        self.root_offsets.insert(root, offset);
    }

    pub fn upload_atlas(&mut self, device: &Device, queue: &Queue, atlas: &mut GlyphAtlas) {
        let pages = atlas.take_dirty_pages();
        for page_index in pages {
            while self.page_textures.len() <= page_index {
                let texture = make_texture(
                    device,
                    "ignit ui atlas page",
                    atlas.page_size(),
                    TextureFormat::R8Unorm,
                    TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                );
                let view = texture.create_view(&TextureViewDescriptor::default());
                let bind_group = make_texture_bind_group(
                    device,
                    &self.texture_layout,
                    &view,
                    &self.atlas_sampler,
                    "ignit ui atlas page bind group",
                );
                self.page_textures.push(texture);
                self.page_bind_groups.push(bind_group);
            }
            let page = &atlas.pages[page_index];
            queue.write_texture(
                TexelCopyTextureInfo {
                    texture: &self.page_textures[page_index],
                    mip_level: 0,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::All,
                },
                &page.pixels,
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(page.size),
                    rows_per_image: Some(page.size),
                },
                Extent3d {
                    width: page.size,
                    height: page.size,
                    depth_or_array_layers: 1,
                },
            );
        }
        self.stats.atlas_pages = atlas.page_count();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        target: &TextureView,
        draw: &UiDrawList,
        text: &mut TextEngine,
        world: &World,
        screen: (f32, f32),
    ) {
        let mut vertices = Vec::new();
        let mut batches: Vec<UiBatch> = Vec::new();
        for command in &draw.commands {
            self.push_command(&mut vertices, &mut batches, command, text, world);
        }
        self.stats.vertices = vertices.len();
        self.stats.batches = batches.len();
        if vertices.is_empty() {
            return;
        }
        let uniform = ScreenUniform {
            size: [screen.0.max(1.0), screen.1.max(1.0)],
            _pad: [0.0; 2],
        };
        queue.write_buffer(&self.screen_buffer, 0, bytemuck::bytes_of(&uniform));

        let bytes = bytemuck::cast_slice(&vertices);
        if bytes.len() as u64 > self.vertex_capacity {
            self.vertex_capacity = (bytes.len() as u64).next_power_of_two();
            self.vertex_buffer = device.create_buffer(&BufferDescriptor {
                label: Some("ignit ui vertices"),
                size: self.vertex_capacity,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        queue.write_buffer(&self.vertex_buffer, 0, bytes);

        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("ignit ui pass"),
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
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        let bounds = UiRect::new(0.0, 0.0, screen.0, screen.1);
        for batch in &batches {
            if batch.count == 0 {
                continue;
            }
            let Some(clip) = batch.clip.intersect(bounds) else {
                continue;
            };
            let bind_group = match batch.texture {
                UiTextureRef::White => &self.white_bind_group,
                UiTextureRef::Atlas(page) => self
                    .page_bind_groups
                    .get(page)
                    .unwrap_or(&self.white_bind_group),
                UiTextureRef::Image(id) => self
                    .image_bind_groups
                    .get(&id)
                    .unwrap_or(&self.white_bind_group),
            };
            pass.set_bind_group(1, bind_group, &[]);
            pass.set_scissor_rect(
                clip.x.floor().max(0.0) as u32,
                clip.y.floor().max(0.0) as u32,
                clip.width.ceil().max(1.0) as u32,
                clip.height.ceil().max(1.0) as u32,
            );
            pass.draw(batch.start..batch.start + batch.count, 0..1);
        }
    }

    fn push_command(
        &self,
        vertices: &mut Vec<UiVertex>,
        batches: &mut Vec<UiBatch>,
        command: &UiDrawCommand,
        text: &mut TextEngine,
        world: &World,
    ) {
        match command {
            UiDrawCommand::Shadow {
                entity,
                rect,
                color,
                offset,
                blur,
                spread,
                radius,
                clip,
            } => {
                let (scale, origin) = self.transform(world, *entity);
                let rect = transform_rect(
                    UiRect::new(
                        rect.x + offset.x - spread,
                        rect.y + offset.y - spread,
                        rect.width + spread * 2.0,
                        rect.height + spread * 2.0,
                    ),
                    scale,
                    origin,
                );
                push_quad(
                    vertices,
                    batches,
                    rect,
                    *color,
                    [0.0, 0.0, 1.0, 1.0],
                    [radius[0] * scale, 4.0, rect.width, rect.height],
                    [blur * scale, 0.0],
                    clip.map(|clip| transform_rect(clip, scale, origin)),
                    UiTextureRef::White,
                );
            }
            UiDrawCommand::Rect {
                entity,
                rect,
                color,
                radius,
                clip,
            } => {
                let (scale, origin) = self.transform(world, *entity);
                let rect = transform_rect(*rect, scale, origin);
                push_quad(
                    vertices,
                    batches,
                    rect,
                    *color,
                    [0.0, 0.0, 1.0, 1.0],
                    [radius[0] * scale, 0.0, rect.width, rect.height],
                    [0.0; 2],
                    clip.map(|clip| transform_rect(clip, scale, origin)),
                    UiTextureRef::White,
                );
            }
            UiDrawCommand::Border {
                entity,
                rect,
                width,
                color,
                radius,
                clip,
            } => {
                let (scale, origin) = self.transform(world, *entity);
                let rect = transform_rect(*rect, scale, origin);
                push_quad(
                    vertices,
                    batches,
                    rect,
                    *color,
                    [0.0, 0.0, 1.0, 1.0],
                    [radius[0] * scale, 3.0, rect.width, rect.height],
                    [width * scale, 0.0],
                    clip.map(|clip| transform_rect(clip, scale, origin)),
                    UiTextureRef::White,
                );
            }
            UiDrawCommand::Line {
                entity,
                rect,
                color,
                clip,
            } => {
                let (scale, origin) = self.transform(world, *entity);
                let rect = transform_rect(*rect, scale, origin);
                push_quad(
                    vertices,
                    batches,
                    rect,
                    *color,
                    [0.0, 0.0, 1.0, 1.0],
                    [0.0, 0.0, rect.width, rect.height],
                    [0.0; 2],
                    clip.map(|clip| transform_rect(clip, scale, origin)),
                    UiTextureRef::White,
                );
            }
            UiDrawCommand::Image {
                entity,
                rect,
                texture,
                tint,
                uv,
                clip,
                ..
            } => {
                let (scale, origin) = self.transform(world, *entity);
                let rect = transform_rect(*rect, scale, origin);
                let texture_ref = self
                    .image_names
                    .get(texture)
                    .filter(|id| self.image_bind_groups.contains_key(id))
                    .map(|id| UiTextureRef::Image(*id))
                    .unwrap_or(UiTextureRef::White);
                push_quad(
                    vertices,
                    batches,
                    rect,
                    *tint,
                    *uv,
                    [0.0, 2.0, rect.width, rect.height],
                    [0.0; 2],
                    clip.map(|clip| transform_rect(clip, scale, origin)),
                    texture_ref,
                );
            }
            UiDrawCommand::Text {
                entity,
                rect,
                text: value,
                style,
                clip,
                caret,
                selection,
            } => {
                if clip.is_some_and(|clip| clip.intersect(*rect).is_none()) {
                    return;
                }
                let (scale, origin) = self.transform(world, *entity);
                let text_style = text_style(style);
                let shaped = text.shape(value, &text_style, rect.width);
                let quads = text.quads(&shaped);
                let clip = clip.map(|clip| transform_rect(clip, scale, origin));
                if let Some((start, end)) = selection
                    && let Some(selection) = text.selection_rect(&shaped, *start, *end)
                {
                    push_quad(
                        vertices,
                        batches,
                        transform_rect(
                            UiRect::new(
                                rect.x + selection[0],
                                rect.y + selection[1],
                                selection[2],
                                selection[3],
                            ),
                            scale,
                            origin,
                        ),
                        [0.32, 0.56, 0.95, 0.35],
                        [0.0; 4],
                        [0.0; 4],
                        [0.0; 2],
                        clip,
                        UiTextureRef::White,
                    );
                }
                for quad in quads {
                    let rect = transform_rect(
                        UiRect::new(
                            rect.x + quad.rect[0],
                            rect.y + quad.rect[1],
                            quad.rect[2],
                            quad.rect[3],
                        ),
                        scale,
                        origin,
                    );
                    push_quad(
                        vertices,
                        batches,
                        rect,
                        quad.color,
                        quad.uv,
                        [0.0, 1.0, rect.width, rect.height],
                        [0.0; 2],
                        clip,
                        UiTextureRef::Atlas(quad.page),
                    );
                }
                if let Some(cursor) = caret
                    && let Some(caret_rect) = text.cursor_rect(&shaped, *cursor)
                {
                    let caret_rect = transform_rect(
                        UiRect::new(
                            rect.x + caret_rect[0],
                            rect.y + caret_rect[1],
                            caret_rect[2],
                            caret_rect[3],
                        ),
                        scale,
                        origin,
                    );
                    push_quad(
                        vertices,
                        batches,
                        caret_rect,
                        [0.92, 0.94, 1.0, 0.9],
                        [0.0, 0.0, 1.0, 1.0],
                        [0.0, 0.0, caret_rect.width, caret_rect.height],
                        [0.0; 2],
                        clip,
                        UiTextureRef::White,
                    );
                }
            }
        }
    }

    fn transform(&self, world: &World, entity: Entity) -> (f32, UiPoint) {
        let mut root = entity;
        let nodes = world.get_component_store::<UiNode>();
        let views = world.get_component_store::<UiView>();
        while let Some(node) = nodes
            .as_ref()
            .and_then(|store| store.get_component(root.index))
        {
            let Some(parent) = node.parent else {
                break;
            };
            root = parent;
        }
        let view = views
            .as_ref()
            .and_then(|store| store.get_component(root.index));
        let scale = view.map(|view| view.scale).unwrap_or(1.0);
        let extra = self.root_offsets.get(&root).copied().unwrap_or_default();
        (scale, extra)
    }
}

pub struct UiLayerRenderer {
    pub renderer: UiRenderer,
    pub text: Arc<Mutex<TextEngine>>,
}

impl UiLayerRenderer {
    pub fn new(
        device: &Device,
        queue: &Queue,
        format: TextureFormat,
        text: Arc<Mutex<TextEngine>>,
    ) -> Self {
        let atlas_page_size = text
            .lock()
            .expect("ui text mutex poisoned")
            .atlas
            .page_size();
        Self {
            renderer: UiRenderer::new_with_atlas_size(device, queue, format, atlas_page_size),
            text,
        }
    }
}

impl LayerRenderer for UiLayerRenderer {
    fn layer(&self) -> RenderLayer {
        RenderLayer::UI
    }

    fn submission(&self) -> OverlaySubmission {
        OverlaySubmission::MainEncoder
    }

    fn debug_name(&self) -> &str {
        "ignit-ui"
    }

    fn render(&mut self, ctx: RenderOverlayContext) {
        let Some(encoder) = ctx.encoder else {
            return;
        };
        let Some(draw) = ctx.ecs.get_resource::<UiDrawList>() else {
            return;
        };
        let mut text = self.text.lock().expect("ui text mutex poisoned");
        text.begin_frame();
        self.renderer
            .upload_atlas(ctx.device, ctx.queue, &mut text.atlas);
        self.renderer.render(
            ctx.device,
            ctx.queue,
            encoder,
            ctx.target_view,
            &draw,
            &mut text,
            ctx.ecs,
            ctx.screen_size,
        );
    }
}

fn text_style(style: &UiResolvedTextStyle) -> UiTextStyle {
    UiTextStyle {
        size: style.size,
        line_height: style.line_height,
        color: ignit_ui::UiColor::Rgba(style.color),
        family: style.family.clone(),
        weight: style.weight,
        wrap: style.wrap,
        ellipsis: style.ellipsis,
    }
}

fn transform_rect(rect: UiRect, scale: f32, origin: UiPoint) -> UiRect {
    UiRect {
        x: origin.x + rect.x * scale,
        y: origin.y + rect.y * scale,
        width: rect.width * scale,
        height: rect.height * scale,
    }
}

#[allow(clippy::too_many_arguments)]
fn push_quad(
    vertices: &mut Vec<UiVertex>,
    batches: &mut Vec<UiBatch>,
    rect: UiRect,
    color: [f32; 4],
    uv: [f32; 4],
    params: [f32; 4],
    aux: [f32; 2],
    clip: Option<UiRect>,
    texture: UiTextureRef,
) {
    let clip = clip.unwrap_or(rect);
    let start = vertices.len() as u32;
    let x = rect.x;
    let y = rect.y;
    let w = rect.width.max(0.0);
    let h = rect.height.max(0.0);
    let quad = [
        ([x, y], [uv[0], uv[1]]),
        ([x + w, y], [uv[2], uv[1]]),
        ([x + w, y + h], [uv[2], uv[3]]),
        ([x, y], [uv[0], uv[1]]),
        ([x + w, y + h], [uv[2], uv[3]]),
        ([x, y + h], [uv[0], uv[3]]),
    ];
    for (position, uv) in quad {
        vertices.push(UiVertex {
            position,
            color,
            uv,
            params,
            aux,
        });
    }
    let mode = params[1] as u32;
    if let Some(last) = batches.last_mut()
        && last.mode == mode
        && last.texture == texture
        && last.clip == clip
    {
        last.count += 6;
        return;
    }
    batches.push(UiBatch {
        start,
        count: 6,
        clip,
        mode,
        texture,
    });
}

fn make_texture(
    device: &Device,
    label: &str,
    size: u32,
    format: TextureFormat,
    usage: wgpu::TextureUsages,
) -> Texture {
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
        usage,
        view_formats: &[],
    })
}

fn make_texture_bind_group(
    device: &Device,
    layout: &BindGroupLayout,
    view: &TextureView,
    sampler: &Sampler,
    label: &str,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(view),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Sampler(sampler),
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> UiRect {
        UiRect::new(x, y, w, h)
    }

    #[test]
    fn ui_shader_validates() {
        let module = wgpu::naga::front::wgsl::parse_str(SHADER).expect("WGSL parses");
        let mut validator = wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        );
        validator.validate(&module).expect("WGSL validates");
    }

    #[test]
    fn quads_merge_into_one_batch_when_state_matches() {
        let mut vertices = Vec::new();
        let mut batches = Vec::new();
        push_quad(
            &mut vertices,
            &mut batches,
            rect(0.0, 0.0, 10.0, 10.0),
            [1.0; 4],
            [0.0, 0.0, 1.0, 1.0],
            [0.0, 0.0, 10.0, 10.0],
            [0.0; 2],
            Some(rect(0.0, 0.0, 100.0, 100.0)),
            UiTextureRef::White,
        );
        push_quad(
            &mut vertices,
            &mut batches,
            rect(20.0, 0.0, 10.0, 10.0),
            [1.0; 4],
            [0.0, 0.0, 1.0, 1.0],
            [0.0, 0.0, 10.0, 10.0],
            [0.0; 2],
            Some(rect(0.0, 0.0, 100.0, 100.0)),
            UiTextureRef::White,
        );
        assert_eq!(vertices.len(), 12);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].count, 12);
    }

    #[test]
    fn clipping_splits_batches() {
        let mut vertices = Vec::new();
        let mut batches = Vec::new();
        for clip in [rect(0.0, 0.0, 10.0, 10.0), rect(0.0, 0.0, 20.0, 20.0)] {
            push_quad(
                &mut vertices,
                &mut batches,
                rect(0.0, 0.0, 10.0, 10.0),
                [1.0; 4],
                [0.0, 0.0, 1.0, 1.0],
                [0.0, 0.0, 10.0, 10.0],
                [0.0; 2],
                Some(clip),
                UiTextureRef::White,
            );
        }
        assert_eq!(batches.len(), 2);
    }

    #[test]
    fn root_transform_scales_and_offsets() {
        let transformed = transform_rect(rect(10.0, 20.0, 30.0, 40.0), 2.0, UiPoint::new(5.0, 7.0));
        assert_eq!(transformed, rect(25.0, 47.0, 60.0, 80.0));
    }
}
