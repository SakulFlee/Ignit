//! Fullscreen debug views for individual render stages.
//!
//! The renderer records this pass last when [`DebugView`] is not `Off`,
//! overwriting the normal post-chain output with a raw stage buffer.

use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBindingType,
    BufferDescriptor, BufferUsages, Color, ColorTargetState, ColorWrites, CommandEncoder, Device,
    FragmentState, LoadOp, Operations, PipelineLayoutDescriptor, PrimitiveState, PrimitiveTopology,
    Queue, RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline,
    RenderPipelineDescriptor, ShaderModuleDescriptor, ShaderSource, ShaderStages, StoreOp,
    TextureSampleType, TextureView, TextureViewDimension, VertexState,
};

/// Render-stage buffer to display instead of the normal final image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DebugView {
    #[default]
    Off,
    SceneColor,
    Depth,
    GbufferAlbedo,
    GbufferNormal,
    GbufferMaterial,
}

impl DebugView {
    pub const ALL: [Self; 6] = [
        Self::Off,
        Self::SceneColor,
        Self::Depth,
        Self::GbufferAlbedo,
        Self::GbufferNormal,
        Self::GbufferMaterial,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::SceneColor => "Scene color",
            Self::Depth => "Depth",
            Self::GbufferAlbedo => "G-buffer albedo",
            Self::GbufferNormal => "G-buffer normal",
            Self::GbufferMaterial => "G-buffer material",
        }
    }

    pub fn is_depth(self) -> bool {
        self == Self::Depth
    }
}

const COLOR_WGSL: &str = include_str!("debug_view_color.wgsl");
const DEPTH_WGSL: &str = include_str!("debug_view_depth.wgsl");

pub(crate) struct DebugViewRenderer {
    color_pipeline: RenderPipeline,
    depth_pipeline: RenderPipeline,
    color_layout: BindGroupLayout,
    depth_layout: BindGroupLayout,
    params: Buffer,
}

impl DebugViewRenderer {
    pub fn new(device: &Device, target_format: wgpu::TextureFormat) -> Self {
        let color_module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Ignit::DebugViewColorShader"),
            source: ShaderSource::Wgsl(COLOR_WGSL.into()),
        });
        let depth_module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Ignit::DebugViewDepthShader"),
            source: ShaderSource::Wgsl(DEPTH_WGSL.into()),
        });

        let color_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Ignit::DebugViewColorLayout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: false },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
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
            ],
        });
        let depth_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Ignit::DebugViewDepthLayout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Depth,
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
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
            ],
        });

        let color_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Ignit::DebugViewColorPipelineLayout"),
            bind_group_layouts: &[Some(&color_layout)],
            immediate_size: 0,
        });
        let depth_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Ignit::DebugViewDepthPipelineLayout"),
            bind_group_layouts: &[Some(&depth_layout)],
            immediate_size: 0,
        });

        let pipeline = |label, layout, module, entry| {
            device.create_render_pipeline(&RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: VertexState {
                    module,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(FragmentState {
                    module,
                    entry_point: Some(entry),
                    targets: &[Some(ColorTargetState {
                        format: target_format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
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
                multisample: Default::default(),
                cache: None,
                multiview_mask: None,
            })
        };

        Self {
            color_pipeline: pipeline(
                "Ignit::DebugViewColorPipeline",
                &color_pipeline_layout,
                &color_module,
                "fs_main",
            ),
            depth_pipeline: pipeline(
                "Ignit::DebugViewDepthPipeline",
                &depth_pipeline_layout,
                &depth_module,
                "fs_main",
            ),
            color_layout,
            depth_layout,
            params: device.create_buffer(&BufferDescriptor {
                label: Some("Ignit::DebugViewParams"),
                size: 16,
                usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &self,
        encoder: &mut CommandEncoder,
        device: &Device,
        queue: &Queue,
        target_view: &TextureView,
        source_view: &TextureView,
        depth: bool,
        target_size: [u32; 2],
    ) {
        let width = target_size[0].max(1) as f32;
        let height = target_size[1].max(1) as f32;
        let mut params = [0u8; 16];
        params[0..4].copy_from_slice(&width.to_le_bytes());
        params[4..8].copy_from_slice(&height.to_le_bytes());
        queue.write_buffer(&self.params, 0, &params);

        let (layout, pipeline) = if depth {
            (&self.depth_layout, &self.depth_pipeline)
        } else {
            (&self.color_layout, &self.color_pipeline)
        };

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Ignit::DebugViewBindGroup"),
            layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(source_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Buffer(self.params.as_entire_buffer_binding()),
                },
            ],
        });

        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("Ignit::DebugView"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_debug_view_has_a_distinct_label() {
        let mut labels = DebugView::ALL
            .iter()
            .map(|view| view.label())
            .collect::<Vec<_>>();
        labels.sort_unstable();
        labels.dedup();

        assert_eq!(labels.len(), DebugView::ALL.len());
    }

    #[test]
    fn only_depth_selects_the_depth_pipeline() {
        assert!(DebugView::Depth.is_depth());
        for view in DebugView::ALL
            .into_iter()
            .filter(|view| *view != DebugView::Depth)
        {
            assert!(!view.is_depth());
        }
    }
}
