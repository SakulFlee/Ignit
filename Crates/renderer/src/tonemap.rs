//! Fullscreen HDR → LDR tonemap pass.

use cgmath::{InnerSpace, Matrix4, Point3, Vector2, Vector3, Vector4};
use wgpu::{
    BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType,
    BlendComponent, BlendState, BufferBindingType, ColorTargetState, ColorWrites, Device, Extent3d,
    FragmentState, PipelineLayoutDescriptor, PrimitiveState, RenderPassColorAttachment,
    RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor, SamplerBindingType,
    SamplerDescriptor, ShaderModuleDescriptor, ShaderSource, ShaderStages, StoreOp,
    TextureDescriptor as WgpuTextureDescriptor, TextureDimension, TextureSampleType, TextureUsages,
    TextureView, TextureViewDimension, VertexState,
    util::{BufferInitDescriptor, DeviceExt},
};

use crate::scene::{GlareLight, MAX_GLARE_LIGHTS};

const TONEMAP_WGSL: &str = include_str!("tonemap.wgsl");

/// Per-frame values for the tonemap and light-glare pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TonemapSettings {
    pub exposure: f32,
    pub bloom_threshold: f32,
    pub bloom_intensity: f32,
    pub contrast: f32,
    pub glare_stars: bool,
    pub glare_star_arms: u32,
    pub glare_star_inner: f32,
    pub glare_star_outer: f32,
    pub glare_star_length: f32,
    pub volumetric_range: f32,
}

impl Default for TonemapSettings {
    fn default() -> Self {
        Self {
            exposure: 1.0,
            bloom_threshold: 4.0,
            bloom_intensity: 0.0,
            contrast: 1.0,
            glare_stars: true,
            glare_star_arms: 8,
            glare_star_inner: 0.05,
            glare_star_outer: 0.0,
            glare_star_length: 10.0,
            volumetric_range: 1.0,
        }
    }
}

/// Matrix (16) + camera position (4) + viewport (4) + camera params (4) +
/// depth resolution (4) + lights (16 each).
const GLARE_PARAMS_FLOATS: usize = 16 + 4 + 4 + 4 + 4 + MAX_GLARE_LIGHTS * 16;
const GLARE_PARAMS_BYTES: usize = GLARE_PARAMS_FLOATS * 4;
const GLARE_RADIUS: f32 = 0.028;
/// Maximum lights evaluated by the per-pixel glare loop. The shader keeps its
/// 32-slot buffer, but only the brightest nearby on-screen lights are written
/// to it.
const GLARE_LIGHT_BUDGET: usize = 8;
const TONEMAP_PARAMS_BYTES: usize = 64;

struct TonemapParams {
    buffer: wgpu::Buffer,
    glare_buffer: wgpu::Buffer,
}

impl TonemapParams {
    fn new(device: &Device, srgb_target: bool) -> Self {
        // An sRGB render target applies the transfer function in hardware.
        let gamma: f32 = if srgb_target { 1.0 } else { 1.0 / 2.2 };
        let settings = TonemapSettings::default();
        let data = tonemap_params_data(&settings, gamma);
        let buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Ignit::TonemapParams"),
            contents: &data,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let glare_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Ignit::GlareParams"),
            contents: &[0u8; GLARE_PARAMS_BYTES],
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            buffer,
            glare_buffer,
        }
    }
}

fn tonemap_params_data(settings: &TonemapSettings, gamma: f32) -> [u8; TONEMAP_PARAMS_BYTES] {
    let mut data = [0u8; TONEMAP_PARAMS_BYTES];
    data[0..4].copy_from_slice(&settings.exposure.to_le_bytes());
    data[4..8].copy_from_slice(&gamma.to_le_bytes());
    data[8..12].copy_from_slice(&settings.bloom_threshold.to_le_bytes());
    data[12..16].copy_from_slice(&settings.bloom_intensity.to_le_bytes());
    data[16..20].copy_from_slice(&settings.contrast.to_le_bytes());
    data[20..24].copy_from_slice(&u32::from(settings.glare_stars).to_le_bytes());
    data[24..28].copy_from_slice(&settings.glare_star_arms.to_le_bytes());
    data[28..32].copy_from_slice(&settings.glare_star_inner.to_le_bytes());
    data[32..36].copy_from_slice(&settings.glare_star_outer.to_le_bytes());
    data[36..40].copy_from_slice(&settings.glare_star_length.to_le_bytes());
    // The volumetric flag is written by `record` from the actual bind view.
    data[44..48].copy_from_slice(&settings.volumetric_range.to_le_bytes());
    data
}

fn write_f32(data: &mut [u8], offset: usize, value: f32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

struct TonemapBindGroupCache {
    hdr: TextureView,
    depth: TextureView,
    volumetric: Option<TextureView>,
    bloom: Option<TextureView>,
    glare: Option<TextureView>,
    bind_group: wgpu::BindGroup,
}

/// Score used to choose which lights deserve a glare pass. Mirrors the
/// shader's inverse-quadratic attenuation.
fn glare_score(light: &GlareLight, camera_position: &Point3<f32>) -> f32 {
    let offset = Vector3::new(
        light.position.x - camera_position.x,
        light.position.y - camera_position.y,
        light.position.z - camera_position.z,
    );
    light.intensity / (1.0 + offset.dot(offset) * 0.015)
}

/// Keeps the brightest nearby lights whose glare could reach the screen.
fn rank_glare_lights<'a>(
    lights: &'a [GlareLight],
    view_projection: &Matrix4<f32>,
    camera_position: &Point3<f32>,
) -> Vec<&'a GlareLight> {
    let mut visible: Vec<&GlareLight> = lights
        .iter()
        .filter(|light| {
            let clip = view_projection
                * Vector4::new(light.position.x, light.position.y, light.position.z, 1.0);
            if clip.w <= 0.0 {
                return false;
            }
            let x = clip.x / clip.w;
            let y = clip.y / clip.w;
            x.abs() <= 1.2 && y.abs() <= 1.2
        })
        .collect();

    visible.sort_by(|a, b| {
        glare_score(b, camera_position).total_cmp(&glare_score(a, camera_position))
    });
    visible.truncate(GLARE_LIGHT_BUDGET);
    visible
}

/// Owns the fullscreen tonemap pipeline and its static resources.
pub struct Tonemap {
    pipeline: RenderPipeline,
    bind_group_layout: BindGroupLayout,
    sampler: wgpu::Sampler,
    params: TonemapParams,
    gamma: f32,
    _volumetric_fallback: wgpu::Texture,
    volumetric_fallback_view: TextureView,
    _bloom_fallback: wgpu::Texture,
    bloom_fallback_view: TextureView,
    bloom_bright_pipeline: RenderPipeline,
    bloom_downsample_pipeline: RenderPipeline,
    bloom_upsample_pipeline: RenderPipeline,
    bloom_layout: BindGroupLayout,
    _glare_fallback: wgpu::Texture,
    glare_fallback_view: TextureView,
    glare_pipeline: RenderPipeline,
    glare_layout: BindGroupLayout,
    bind_group_cache: Option<TonemapBindGroupCache>,
}

impl Tonemap {
    pub fn new(
        device: &Device,
        target_format: wgpu::TextureFormat,
        intermediate_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Ignit::TonemapShader"),
            source: ShaderSource::Wgsl(TONEMAP_WGSL.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Ignit::TonemapBindGroupLayout"),
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
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Depth,
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 5,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 6,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 7,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 8,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Ignit::TonemapPipelineLayout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let targets = [Some(ColorTargetState {
            format: target_format,
            blend: Some(BlendState::REPLACE),
            write_mask: ColorWrites::ALL,
        })];

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Ignit::TonemapPipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &targets,
                compilation_options: Default::default(),
            }),
            depth_stencil: None,
            primitive: PrimitiveState::default(),
            cache: None,
            multisample: Default::default(),
            multiview_mask: None,
        });

        let bloom_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Ignit::BloomShader"),
            source: ShaderSource::Wgsl(include_str!("bloom.wgsl").into()),
        });
        let bloom_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Ignit::BloomBindGroupLayout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bloom_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Ignit::BloomPipelineLayout"),
            bind_group_layouts: &[Some(&bloom_layout)],
            immediate_size: 0,
        });
        let bloom_replace_targets = [Some(ColorTargetState {
            format: intermediate_format,
            blend: Some(BlendState::REPLACE),
            write_mask: ColorWrites::ALL,
        })];
        // The upsample pass accumulates into the larger level in place.
        let bloom_additive = BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let bloom_add_targets = [Some(ColorTargetState {
            format: intermediate_format,
            blend: Some(BlendState {
                color: bloom_additive,
                alpha: bloom_additive,
            }),
            write_mask: ColorWrites::ALL,
        })];
        let create_bloom =
            |label: &'static str, fragment: &'static str, targets: &[Option<ColorTargetState>]| {
                device.create_render_pipeline(&RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&bloom_pipeline_layout),
                    vertex: VertexState {
                        module: &bloom_shader,
                        entry_point: Some("vs_main"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(FragmentState {
                        module: &bloom_shader,
                        entry_point: Some(fragment),
                        targets,
                        compilation_options: Default::default(),
                    }),
                    depth_stencil: None,
                    primitive: PrimitiveState::default(),
                    cache: None,
                    multisample: Default::default(),
                    multiview_mask: None,
                })
            };
        let bloom_bright_pipeline = create_bloom(
            "Ignit::BloomBrightPipeline",
            "fs_bright",
            &bloom_replace_targets,
        );
        let bloom_downsample_pipeline = create_bloom(
            "Ignit::BloomDownsamplePipeline",
            "fs_downsample",
            &bloom_replace_targets,
        );
        let bloom_upsample_pipeline = create_bloom(
            "Ignit::BloomUpsamplePipeline",
            "fs_upsample",
            &bloom_add_targets,
        );

        let glare_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Ignit::GlareShader"),
            source: ShaderSource::Wgsl(include_str!("glare.wgsl").into()),
        });
        let glare_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Ignit::GlareBindGroupLayout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Depth,
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let glare_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Ignit::GlarePipelineLayout"),
            bind_group_layouts: &[Some(&glare_layout)],
            immediate_size: 0,
        });
        let glare_targets = [Some(ColorTargetState {
            format: intermediate_format,
            blend: Some(BlendState::REPLACE),
            write_mask: ColorWrites::ALL,
        })];
        let glare_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Ignit::GlarePipeline"),
            layout: Some(&glare_pipeline_layout),
            vertex: VertexState {
                module: &glare_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(FragmentState {
                module: &glare_shader,
                entry_point: Some("fs_main"),
                targets: &glare_targets,
                compilation_options: Default::default(),
            }),
            depth_stencil: None,
            primitive: PrimitiveState::default(),
            cache: None,
            multisample: Default::default(),
            multiview_mask: None,
        });

        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("Ignit::TonemapSampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let gamma: f32 = if target_format.is_srgb() {
            1.0
        } else {
            1.0 / 2.2
        };
        let params = TonemapParams::new(device, target_format.is_srgb());

        // Tonemap always binds a 3D volume so the layout is fixed. The shader
        // ignores it while `volumetric_enabled` is clear.
        let volumetric_fallback = device.create_texture(&WgpuTextureDescriptor {
            label: Some("Ignit::TonemapVolumetricFallback"),
            size: Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let volumetric_fallback_view =
            volumetric_fallback.create_view(&wgpu::TextureViewDescriptor::default());

        let bloom_fallback = device.create_texture(&WgpuTextureDescriptor {
            label: Some("Ignit::TonemapBloomFallback"),
            size: Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let bloom_fallback_view =
            bloom_fallback.create_view(&wgpu::TextureViewDescriptor::default());

        let glare_fallback = device.create_texture(&WgpuTextureDescriptor {
            label: Some("Ignit::TonemapGlareFallback"),
            size: Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let glare_fallback_view =
            glare_fallback.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            params,
            gamma,
            _volumetric_fallback: volumetric_fallback,
            volumetric_fallback_view,
            _bloom_fallback: bloom_fallback,
            bloom_fallback_view,
            bloom_bright_pipeline,
            bloom_downsample_pipeline,
            bloom_upsample_pipeline,
            bloom_layout,
            _glare_fallback: glare_fallback,
            glare_fallback_view,
            glare_pipeline,
            glare_layout,
            bind_group_cache: None,
        }
    }

    /// Update exposure, bloom, contrast, and glare-star parameters.
    pub fn set_params(&self, queue: &wgpu::Queue, settings: &TonemapSettings) {
        let data = tonemap_params_data(settings, self.gamma);
        queue.write_buffer(&self.params.buffer, 0, &data);
    }

    /// Update the camera and positional-light data used for screen-space glare.
    ///
    /// `output_resolution` is the target of this tonemap pass. With
    /// anti-aliasing enabled it is the internal render resolution; with AA
    /// disabled it is the native surface resolution. `depth_resolution` is the
    /// resolution of the scene depth texture, which may be smaller than
    /// `output_resolution` when render scaling is active.
    #[allow(clippy::too_many_arguments)]
    pub fn set_glare(
        &self,
        queue: &wgpu::Queue,
        view_projection: Matrix4<f32>,
        camera_position: Point3<f32>,
        output_resolution: Vector2<u32>,
        depth_resolution: Vector2<u32>,
        near: f32,
        far: f32,
        lights: &[GlareLight],
    ) {
        let mut data = [0u8; GLARE_PARAMS_BYTES];

        for (column, vector) in [
            view_projection.x,
            view_projection.y,
            view_projection.z,
            view_projection.w,
        ]
        .iter()
        .enumerate()
        {
            for (row, value) in [vector.x, vector.y, vector.z, vector.w].iter().enumerate() {
                write_f32(&mut data, (column * 4 + row) * 4, *value);
            }
        }

        write_f32(&mut data, 64, camera_position.x);
        write_f32(&mut data, 68, camera_position.y);
        write_f32(&mut data, 72, camera_position.z);

        let selected = rank_glare_lights(lights, &view_projection, &camera_position);

        write_f32(&mut data, 80, output_resolution.x as f32);
        write_f32(&mut data, 84, output_resolution.y as f32);
        write_f32(&mut data, 88, selected.len() as f32);
        write_f32(&mut data, 92, GLARE_RADIUS);
        write_f32(&mut data, 96, near);
        write_f32(&mut data, 100, far);
        write_f32(&mut data, 112, depth_resolution.x.max(1) as f32);
        write_f32(&mut data, 116, depth_resolution.y.max(1) as f32);

        for (index, light) in selected.iter().enumerate() {
            let offset = 128 + index * 64;
            write_f32(&mut data, offset, light.position.x);
            write_f32(&mut data, offset + 4, light.position.y);
            write_f32(&mut data, offset + 8, light.position.z);
            write_f32(&mut data, offset + 16, light.color.x);
            write_f32(&mut data, offset + 20, light.color.y);
            write_f32(&mut data, offset + 24, light.color.z);
            write_f32(&mut data, offset + 28, light.intensity);
            write_f32(&mut data, offset + 32, light.direction.x);
            write_f32(&mut data, offset + 36, light.direction.y);
            write_f32(&mut data, offset + 40, light.direction.z);
            write_f32(
                &mut data,
                offset + 44,
                if light.is_spot { 1.0 } else { 0.0 },
            );
            write_f32(&mut data, offset + 48, light.outer_cone_angle);
            write_f32(&mut data, offset + 52, light.seed);
        }

        queue.write_buffer(&self.params.glare_buffer, 0, &data);
    }

    /// Build the bloom chain from `source` into `targets`.
    ///
    /// `targets[0]` receives the thresholded scene at half resolution; each
    /// following target is half the size of the previous one. The chain is
    /// box-filtered on the way down and tent-filtered back up with additive
    /// blending, so `targets[0]` ends up holding the accumulated glow that the
    /// tonemap samples.
    pub fn record_bloom(
        &self,
        device: &Device,
        encoder: &mut wgpu::CommandEncoder,
        source: &TextureView,
        targets: &[&TextureView],
    ) {
        let Some((bright, levels)) = targets.split_first() else {
            return;
        };
        self.record_bloom_pass(
            device,
            encoder,
            "RenderPass::BloomBright",
            &self.bloom_bright_pipeline,
            source,
            bright,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        );

        for (index, target) in levels.iter().copied().enumerate() {
            self.record_bloom_pass(
                device,
                encoder,
                "RenderPass::BloomDownsample",
                &self.bloom_downsample_pipeline,
                targets[index],
                target,
                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            );
        }

        for index in (0..targets.len() - 1).rev() {
            self.record_bloom_pass(
                device,
                encoder,
                "RenderPass::BloomUpsample",
                &self.bloom_upsample_pipeline,
                targets[index + 1],
                targets[index],
                wgpu::LoadOp::Load,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn record_bloom_pass(
        &self,
        device: &Device,
        encoder: &mut wgpu::CommandEncoder,
        label: &'static str,
        pipeline: &RenderPipeline,
        source: &TextureView,
        target: &TextureView,
        load: wgpu::LoadOp<wgpu::Color>,
    ) {
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Ignit::BloomBindGroup"),
            layout: &self.bloom_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(
                        self.params.buffer.as_entire_buffer_binding(),
                    ),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load,
                    store: StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Render the screen-space light glare into a half-resolution target that
    /// the tonemap samples once per pixel.
    pub fn record_glare(
        &self,
        device: &Device,
        encoder: &mut wgpu::CommandEncoder,
        depth: &TextureView,
        target: &TextureView,
    ) {
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Ignit::GlareBindGroup"),
            layout: &self.glare_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(
                        self.params.buffer.as_entire_buffer_binding(),
                    ),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(
                        self.params.glare_buffer.as_entire_buffer_binding(),
                    ),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("RenderPass::Glare"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.glare_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &mut self,
        device: &Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        hdr_view: &TextureView,
        depth_view: &TextureView,
        target_view: &TextureView,
        volumetric_view: Option<&TextureView>,
        bloom_view: Option<&TextureView>,
        glare_view: Option<&TextureView>,
    ) {
        // Toggle the fog path from the view that is actually bound. This keeps
        // the fallback texture from being sampled when integration did not run.
        queue.write_buffer(
            &self.params.buffer,
            40,
            &u32::from(volumetric_view.is_some()).to_le_bytes(),
        );
        let cache_hit = self.bind_group_cache.as_ref().is_some_and(|cache| {
            cache.hdr == *hdr_view
                && cache.depth == *depth_view
                && cache.volumetric.as_ref() == volumetric_view
                && cache.bloom.as_ref() == bloom_view
                && cache.glare.as_ref() == glare_view
        });
        if !cache_hit {
            let volume_view = volumetric_view.unwrap_or(&self.volumetric_fallback_view);
            let bloom_texture = bloom_view.unwrap_or(&self.bloom_fallback_view);
            let glare_texture = glare_view.unwrap_or(&self.glare_fallback_view);
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Ignit::TonemapBindGroup"),
                layout: &self.bind_group_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(hdr_view),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(
                            self.params.buffer.as_entire_buffer_binding(),
                        ),
                    },
                    BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(depth_view),
                    },
                    BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Buffer(
                            self.params.glare_buffer.as_entire_buffer_binding(),
                        ),
                    },
                    BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(volume_view),
                    },
                    BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::TextureView(bloom_texture),
                    },
                    BindGroupEntry {
                        binding: 8,
                        resource: wgpu::BindingResource::TextureView(glare_texture),
                    },
                ],
            });
            self.bind_group_cache = Some(TonemapBindGroupCache {
                hdr: hdr_view.clone(),
                depth: depth_view.clone(),
                volumetric: volumetric_view.cloned(),
                bloom: bloom_view.cloned(),
                glare: glare_view.cloned(),
                bind_group,
            });
        }
        let bind_group = &self
            .bind_group_cache
            .as_ref()
            .expect("tonemap bind group cache")
            .bind_group;

        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("RenderPass::Tonemap"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tonemap_params_pack_exposure_bloom_contrast_and_stars() {
        let settings = TonemapSettings {
            exposure: 2.0,
            bloom_threshold: 0.75,
            bloom_intensity: 1.5,
            contrast: 1.25,
            glare_stars: true,
            glare_star_arms: 6,
            glare_star_inner: 0.08,
            glare_star_outer: 0.03,
            glare_star_length: 7.5,
            volumetric_range: 24.0,
        };
        let data = tonemap_params_data(&settings, 0.5);

        assert_eq!(data.len(), TONEMAP_PARAMS_BYTES);
        assert_eq!(f32::from_le_bytes(data[0..4].try_into().unwrap()), 2.0);
        assert_eq!(f32::from_le_bytes(data[4..8].try_into().unwrap()), 0.5);
        assert_eq!(f32::from_le_bytes(data[8..12].try_into().unwrap()), 0.75);
        assert_eq!(f32::from_le_bytes(data[12..16].try_into().unwrap()), 1.5);
        assert_eq!(f32::from_le_bytes(data[16..20].try_into().unwrap()), 1.25);
        assert_eq!(u32::from_le_bytes(data[20..24].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(data[24..28].try_into().unwrap()), 6);
        assert_eq!(f32::from_le_bytes(data[28..32].try_into().unwrap()), 0.08);
        assert_eq!(f32::from_le_bytes(data[32..36].try_into().unwrap()), 0.03);
        assert_eq!(f32::from_le_bytes(data[36..40].try_into().unwrap()), 7.5);
        assert_eq!(u32::from_le_bytes(data[40..44].try_into().unwrap()), 0);
        assert_eq!(f32::from_le_bytes(data[44..48].try_into().unwrap()), 24.0);
        assert!(data[48..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn glare_ranking_prefers_bright_near_lights_and_caps() {
        let make = |x: f32, intensity: f32| GlareLight {
            position: Vector3::new(x, 0.0, 0.0),
            color: Vector3::new(1.0, 1.0, 1.0),
            intensity,
            direction: Vector3::new(0.0, 0.0, 0.0),
            is_spot: false,
            outer_cone_angle: 0.0,
            seed: 0.0,
        };
        let mut lights: Vec<_> = (0..12)
            .map(|index| make(index as f32 / 12.0, (index + 1) as f32))
            .collect();
        lights.push(make(3.0, 1000.0));

        let camera = Point3::new(0.0, 0.0, 0.0);
        let ranked = rank_glare_lights(&lights, &Matrix4::from_scale(1.0), &camera);

        assert_eq!(ranked.len(), GLARE_LIGHT_BUDGET);
        assert!(ranked.iter().all(|light| light.position.x.abs() <= 1.2));
        assert!(ranked.iter().all(|light| light.intensity < 1000.0));
        let scores: Vec<_> = ranked
            .iter()
            .map(|light| glare_score(light, &camera))
            .collect();
        assert!(scores.windows(2).all(|pair| pair[0] >= pair[1]));
    }
}
