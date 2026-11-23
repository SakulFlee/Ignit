//! Froxel volumetric lighting and height fog.

use std::sync::Arc;

use cgmath::{Matrix4, Vector2};
use ignit_ecs_bridge::VolumetricSettings;
use ignit_shader_preprocessor::{NodeRegistry, ShaderBuilder};
use wgpu::{
    BindGroup, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingResource, BindingType, Buffer, BufferDescriptor, BufferUsages, ComputePassDescriptor,
    ComputePipeline, ComputePipelineDescriptor, Device, PipelineLayoutDescriptor, Queue, Sampler,
    SamplerBindingType, SamplerDescriptor, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    StorageTextureAccess, TextureDescriptor, TextureDimension, TextureSampleType, TextureUsages,
    TextureView, TextureViewDimension,
};

const INJECT_NODES: &[&str] = &[
    "camera_uniform",
    "light_struct",
    "light_types",
    "shadow_types",
    "shadow_slot",
    "slope_scaled_bias",
    "spot_bias_scale",
    "sample_shadow_2d_pcf",
    "compute_shadow_for_light",
];

const INJECT_PARAMS_SIZE: u64 = 64;
const ACCUM_PARAMS_SIZE: u64 = 160;
const INTEGRATE_PARAMS_SIZE: u64 = 32;

/// Temporal blend weight for valid history. Volumetric fog is low-frequency;
/// a high history weight removes the per-frame froxel shimmer without needing
/// a full-screen TAA pass.
const HISTORY_BLEND: f32 = 0.9;

pub struct VolumetricRenderer {
    inject_pipeline: ComputePipeline,
    inject_layout: BindGroupLayout,
    accumulate_pipeline: ComputePipeline,
    accumulate_layout: BindGroupLayout,
    integrate_pipeline: ComputePipeline,
    integrate_layout: BindGroupLayout,
    froxel_sampler: Sampler,
    inject_params: Buffer,
    accum_params: Buffer,
    integrate_params: Buffer,
    grid: [u32; 3],
    integration_steps: u32,
    inject_bind_group: Option<(TextureView, BindGroup)>,
    integrate_bind_group: Option<IntegrateBindGroupCache>,
    prefix: Option<PrefixTargets>,
    history: Option<HistoryTargets>,
    history_index: usize,
    history_valid: bool,
    history_grid: Option<[u32; 3]>,
    history_resolution: Option<Vector2<u32>>,
    current_view: Option<Matrix4<f32>>,
    current_view_projection: Option<Matrix4<f32>>,
    previous_view: Option<Matrix4<f32>>,
    previous_view_projection: Option<Matrix4<f32>>,
    frame_index: u32,
    current_max_distance: f32,
    previous_max_distance: f32,
    temporal_enabled: bool,
}

struct HistoryTargets {
    _textures: [wgpu::Texture; 2],
    views: [TextureView; 2],
}

impl HistoryTargets {
    fn new(device: &Device, grid: [u32; 3]) -> Self {
        let make = |label: &'static str| {
            let texture = device.create_texture(&TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: grid[0],
                    height: grid[1],
                    depth_or_array_layers: grid[2],
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D3,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            (texture, view)
        };
        let (texture0, view0) = make("Ignit::VolumetricHistory0");
        let (texture1, view1) = make("Ignit::VolumetricHistory1");
        Self {
            _textures: [texture0, texture1],
            views: [view0, view1],
        }
    }
}

/// Cumulative in-scattering and transmittance along each froxel column.
///
/// The depth dimension stores prefixes at the integration-step resolution,
/// which is at least the froxel grid depth. The tonemap then needs one
/// texture fetch per pixel instead of integrating the volume per pixel.
struct PrefixTargets {
    _texture: wgpu::Texture,
    view: TextureView,
    grid: [u32; 3],
    steps: u32,
}

impl PrefixTargets {
    fn new(device: &Device, grid: [u32; 3], steps: u32) -> Self {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("Ignit::VolumetricPrefix"),
            size: wgpu::Extent3d {
                width: grid[0],
                height: grid[1],
                depth_or_array_layers: steps,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        Self {
            _texture: texture,
            view,
            grid,
            steps,
        }
    }
}

struct IntegrateBindGroupCache {
    froxel: TextureView,
    prefix: TextureView,
    bind_group: BindGroup,
}

impl VolumetricRenderer {
    pub fn new(device: &Device, queue: &Queue, world_layout: &BindGroupLayout) -> Self {
        let mut registry = NodeRegistry::new();
        registry
            .register_library(&ignit_shader_math::math_library())
            .expect("math node library");
        registry
            .register_library(&ignit_shader_engine::engine_library())
            .expect("engine node library");
        registry
            .register_library(&ignit_shader_pbr::pbr_library())
            .expect("PBR node library");

        let mut builder = ShaderBuilder::new(Arc::new(registry));
        builder
            .add_nodes(INJECT_NODES)
            .expect("volumetric inject nodes");
        builder.add_source(include_str!("volumetric_inject.wgsl"));
        let inject_source = builder.build();
        let inject_module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Ignit::VolumetricInjectShader"),
            source: ShaderSource::Wgsl(inject_source.into()),
        });

        let mut accumulate_builder = ShaderBuilder::new(Arc::new({
            let mut registry = NodeRegistry::new();
            registry
                .register_library(&ignit_shader_math::math_library())
                .expect("math node library");
            registry
                .register_library(&ignit_shader_engine::engine_library())
                .expect("engine node library");
            registry
        }));
        accumulate_builder
            .add_nodes(&["camera_uniform"])
            .expect("volumetric accumulate nodes");
        accumulate_builder.add_source(include_str!("volumetric_accumulate.wgsl"));
        let accumulate_source = accumulate_builder.build();
        let accumulate_module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Ignit::VolumetricAccumulateShader"),
            source: ShaderSource::Wgsl(accumulate_source.into()),
        });

        let integrate_module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Ignit::VolumetricIntegrateShader"),
            source: ShaderSource::Wgsl(include_str!("volumetric_integrate.wgsl").into()),
        });

        let inject_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Ignit::VolumetricInjectLayout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: TextureViewDimension::D3,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let inject_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Ignit::VolumetricInjectPipelineLayout"),
            bind_group_layouts: &[Some(world_layout), Some(&inject_layout)],
            immediate_size: 0,
        });

        let inject_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("Ignit::VolumetricInjectPipeline"),
            layout: Some(&inject_pipeline_layout),
            module: &inject_module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let accumulate_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Ignit::VolumetricAccumulateLayout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: TextureViewDimension::D3,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let accumulate_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Ignit::VolumetricAccumulatePipelineLayout"),
            bind_group_layouts: &[Some(world_layout), Some(&accumulate_layout)],
            immediate_size: 0,
        });

        let accumulate_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("Ignit::VolumetricAccumulatePipeline"),
            layout: Some(&accumulate_pipeline_layout),
            module: &accumulate_module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let integrate_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Ignit::VolumetricIntegrateLayout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: TextureViewDimension::D3,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let integrate_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Ignit::VolumetricIntegratePipelineLayout"),
            bind_group_layouts: &[Some(&integrate_layout)],
            immediate_size: 0,
        });

        let integrate_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("Ignit::VolumetricIntegratePipeline"),
            layout: Some(&integrate_pipeline_layout),
            module: &integrate_module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let froxel_sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("Ignit::VolumetricFroxelSampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let inject_params = device.create_buffer(&BufferDescriptor {
            label: Some("Ignit::VolumetricInjectParams"),
            size: INJECT_PARAMS_SIZE,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let integrate_params = device.create_buffer(&BufferDescriptor {
            label: Some("Ignit::VolumetricIntegrateParams"),
            size: INTEGRATE_PARAMS_SIZE,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let accum_params = device.create_buffer(&BufferDescriptor {
            label: Some("Ignit::VolumetricAccumulateParams"),
            size: ACCUM_PARAMS_SIZE,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut renderer = Self {
            inject_pipeline,
            inject_layout,
            accumulate_pipeline,
            accumulate_layout,
            integrate_pipeline,
            integrate_layout,
            froxel_sampler,
            inject_params,
            accum_params,
            integrate_params,
            grid: [1, 1, 1],
            integration_steps: 1,
            inject_bind_group: None,
            integrate_bind_group: None,
            prefix: None,
            history: None,
            history_index: 0,
            history_valid: false,
            history_grid: None,
            history_resolution: None,
            current_view: None,
            current_view_projection: None,
            previous_view: None,
            previous_view_projection: None,
            frame_index: 0,
            current_max_distance: 1.0,
            previous_max_distance: 0.0,
            temporal_enabled: true,
        };
        renderer.prepare(
            queue,
            &VolumetricSettings::default(),
            Vector2::new(1, 1),
            0,
            None,
            None,
        );
        renderer
    }

    /// Drop temporal history when volumetrics are disabled or invalidated by
    /// an external event such as a camera cut.
    pub fn reset_history(&mut self) {
        self.history = None;
        self.history_index = 0;
        self.history_valid = false;
        self.history_grid = None;
        self.history_resolution = None;
        self.previous_view = None;
        self.previous_view_projection = None;
        self.previous_max_distance = 0.0;
    }

    pub fn prepare(
        &mut self,
        queue: &Queue,
        settings: &VolumetricSettings,
        resolution: Vector2<u32>,
        frame_index: u64,
        current_view: Option<Matrix4<f32>>,
        current_view_projection: Option<Matrix4<f32>>,
    ) {
        // Fixed slice counts make a longer max distance coarser in world
        // space. `resolved_grid` scales the depth resolution with the
        // requested range so short spot cones keep the same sampling density.
        self.grid =
            settings
                .quality
                .resolved_grid(resolution.x, resolution.y, settings.max_distance);
        // Integrate at least at the froxel grid's native depth resolution so
        // the tonemap does not undersample the volume it reads from.
        self.integration_steps =
            settings
                .quality
                .integration_steps(resolution.x, resolution.y, settings.max_distance);
        self.current_max_distance = settings.max_distance.max(0.1);
        self.frame_index = frame_index as u32;
        self.current_view = current_view;
        self.current_view_projection = current_view_projection;
        self.temporal_enabled = settings.temporal_history;
        if !self.temporal_enabled {
            self.reset_history();
        }

        let camera_cut = match (self.previous_view, current_view) {
            (Some(previous), Some(current)) => matrix_max_delta(&previous, &current) > 1.0,
            _ => false,
        };
        let history_invalid = !self.temporal_enabled
            || self.history_grid != Some(self.grid)
            || self.history_resolution != Some(resolution)
            || (self.current_max_distance - self.previous_max_distance).abs() > 1.0e-3
            || current_view.is_none()
            || current_view_projection.is_none()
            || camera_cut;
        if history_invalid {
            self.history = None;
            self.history_index = 0;
            self.history_valid = false;
            self.previous_view = None;
            self.previous_view_projection = None;
        }
        self.history_grid = Some(self.grid);
        self.history_resolution = Some(resolution);

        let mut inject = [0u8; INJECT_PARAMS_SIZE as usize];
        for (index, value) in self.grid.iter().enumerate() {
            inject[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        inject[12..16].copy_from_slice(&self.frame_index.to_le_bytes());
        let media = [
            settings.density,
            settings.scattering,
            settings.extinction,
            settings.anisotropy,
        ];
        for (index, value) in media.iter().enumerate() {
            let offset = 16 + index * 4;
            inject[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        let height = [
            settings.height_base,
            settings.height_falloff,
            settings.height_density,
            settings.max_distance,
        ];
        for (index, value) in height.iter().enumerate() {
            let offset = 32 + index * 4;
            inject[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        for (index, value) in settings.ambient_color.iter().enumerate() {
            let offset = 48 + index * 4;
            inject[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        // `ambient.w` carries the environment/IBL contribution multiplier.
        inject[60..64].copy_from_slice(&settings.environment_intensity.to_le_bytes());
        queue.write_buffer(&self.inject_params, 0, &inject);
    }

    pub fn grid(&self) -> [u32; 3] {
        self.grid
    }

    pub fn record_inject(
        &mut self,
        device: &Device,
        encoder: &mut wgpu::CommandEncoder,
        world_bind_group: &BindGroup,
        froxel_view: &TextureView,
    ) {
        let cache_hit = self
            .inject_bind_group
            .as_ref()
            .is_some_and(|(view, _)| view == froxel_view);

        if !cache_hit {
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Ignit::VolumetricInjectBindGroup"),
                layout: &self.inject_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::TextureView(froxel_view),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: self.inject_params.as_entire_binding(),
                    },
                ],
            });
            self.inject_bind_group = Some((froxel_view.clone(), bind_group));
        }

        let bind_group = &self
            .inject_bind_group
            .as_ref()
            .expect("volumetric inject bind group cache")
            .1;

        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("Volumetric Inject"),
            ..Default::default()
        });
        pass.set_pipeline(&self.inject_pipeline);
        pass.set_bind_group(0, world_bind_group, &[]);
        pass.set_bind_group(1, bind_group, &[]);
        pass.dispatch_workgroups(
            self.grid[0].div_ceil(4),
            self.grid[1].div_ceil(4),
            self.grid[2].div_ceil(4),
        );
    }

    /// Accumulate the current froxel volume with reprojected history.
    ///
    /// Returns the newly accumulated texture view, or `None` when the current
    /// camera is not available (in which case the caller should use the
    /// current froxel volume directly).
    pub fn record_accumulate(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        world_bind_group: &BindGroup,
        current_froxels: &TextureView,
    ) -> Option<TextureView> {
        if !self.temporal_enabled {
            return None;
        }
        let current_view = self.current_view?;
        let current_view_projection = self.current_view_projection?;

        if self.history.is_none() {
            self.history = Some(HistoryTargets::new(device, self.grid));
            self.history_index = 0;
            self.history_valid = false;
        }

        let write_index = self.history_index;
        let read_index = 1 - write_index;
        let history_views = &self.history.as_ref()?.views;
        let history_view = &history_views[read_index];
        let output_view = &history_views[write_index];

        let history_valid = self.history_valid
            && self.previous_view.is_some()
            && self.previous_view_projection.is_some();
        let previous_view = self.previous_view.unwrap_or(current_view);
        let previous_view_projection = self
            .previous_view_projection
            .unwrap_or(current_view_projection);

        let mut params = [0u8; ACCUM_PARAMS_SIZE as usize];
        params[0..64].copy_from_slice(&matrix_to_bytes(&previous_view));
        params[64..128].copy_from_slice(&matrix_to_bytes(&previous_view_projection));
        for (index, value) in self.grid.iter().enumerate() {
            let offset = 128 + index * 4;
            params[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        params[144..148].copy_from_slice(&self.current_max_distance.to_le_bytes());
        params[148..152].copy_from_slice(&self.previous_max_distance.to_le_bytes());
        params[152..156].copy_from_slice(&HISTORY_BLEND.to_le_bytes());
        params[156..160]
            .copy_from_slice(&(if history_valid { 1.0f32 } else { 0.0f32 }).to_le_bytes());
        queue.write_buffer(&self.accum_params, 0, &params);

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Ignit::VolumetricAccumulateBindGroup"),
            layout: &self.accumulate_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(current_froxels),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(history_view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::Sampler(&self.froxel_sampler),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: BindingResource::TextureView(output_view),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: self.accum_params.as_entire_binding(),
                },
            ],
        });

        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("Volumetric Accumulate"),
            ..Default::default()
        });
        pass.set_pipeline(&self.accumulate_pipeline);
        pass.set_bind_group(0, world_bind_group, &[]);
        pass.set_bind_group(1, &bind_group, &[]);
        pass.dispatch_workgroups(
            self.grid[0].div_ceil(4),
            self.grid[1].div_ceil(4),
            self.grid[2].div_ceil(4),
        );
        drop(pass);

        let output = output_view.clone();
        self.history_index = read_index;
        self.history_valid = true;
        self.previous_view = Some(current_view);
        self.previous_view_projection = Some(current_view_projection);
        self.previous_max_distance = self.current_max_distance;

        Some(output)
    }

    /// Pre-integrate the froxel column into cumulative in-scattering and
    /// transmittance so the fullscreen composite only needs one fetch.
    ///
    /// Returns the prefix texture view consumed by the tonemap pass.
    pub fn record_integrate(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        froxel_view: &TextureView,
    ) -> TextureView {
        let needs_target = self.prefix.as_ref().is_none_or(|prefix| {
            prefix.grid != self.grid || prefix.steps != self.integration_steps
        });
        if needs_target {
            self.prefix = Some(PrefixTargets::new(
                device,
                self.grid,
                self.integration_steps,
            ));
            self.integrate_bind_group = None;
        }
        let prefix = self.prefix.as_ref().expect("volumetric prefix target");

        let mut params = [0u8; INTEGRATE_PARAMS_SIZE as usize];
        for (index, value) in self.grid.iter().enumerate() {
            let offset = index * 4;
            params[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        params[12..16].copy_from_slice(&self.integration_steps.to_le_bytes());
        params[16..20].copy_from_slice(&self.current_max_distance.to_le_bytes());
        queue.write_buffer(&self.integrate_params, 0, &params);

        let cache_hit = self
            .integrate_bind_group
            .as_ref()
            .is_some_and(|cache| cache.froxel == *froxel_view && cache.prefix == prefix.view);
        if !cache_hit {
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Ignit::VolumetricIntegrateBindGroup"),
                layout: &self.integrate_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::TextureView(froxel_view),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::TextureView(&prefix.view),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: BindingResource::Sampler(&self.froxel_sampler),
                    },
                    BindGroupEntry {
                        binding: 3,
                        resource: self.integrate_params.as_entire_binding(),
                    },
                ],
            });
            self.integrate_bind_group = Some(IntegrateBindGroupCache {
                froxel: froxel_view.clone(),
                prefix: prefix.view.clone(),
                bind_group,
            });
        }
        let bind_group = &self
            .integrate_bind_group
            .as_ref()
            .expect("volumetric integrate bind group cache")
            .bind_group;

        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("Volumetric Integrate"),
            ..Default::default()
        });
        pass.set_pipeline(&self.integrate_pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.dispatch_workgroups(self.grid[0].div_ceil(4), self.grid[1].div_ceil(4), 1);

        prefix.view.clone()
    }
}

fn matrix_max_delta(a: &Matrix4<f32>, b: &Matrix4<f32>) -> f32 {
    let mut max_delta: f32 = 0.0;
    for (left, right) in [a.x, a.y, a.z, a.w].into_iter().zip([b.x, b.y, b.z, b.w]) {
        for (left, right) in [left.x, left.y, left.z, left.w]
            .into_iter()
            .zip([right.x, right.y, right.z, right.w])
        {
            max_delta = max_delta.max((left - right).abs());
        }
    }
    max_delta
}

fn matrix_to_bytes(matrix: &Matrix4<f32>) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    let columns = [matrix.x, matrix.y, matrix.z, matrix.w];
    for (column_index, column) in columns.iter().enumerate() {
        for (row_index, value) in [column.x, column.y, column.z, column.w].iter().enumerate() {
            let offset = (column_index * 4 + row_index) * 4;
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    bytes
}
