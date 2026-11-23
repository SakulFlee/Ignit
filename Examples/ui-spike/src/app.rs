//! Spike app: Taffy + cosmic-text + swash atlas + scissored wgpu overlay.

use std::time::Instant;

use ignit::app::{InputButton, LayerRenderer, Module, RenderLayer};
use ignit::ecs::World;
use ignit::ecs_bridge::{
    CursorPosition, DeviceResource, IcedEventQueue, InputSnapshot, QueueResource,
    SurfaceFormatResource, TotalTime,
};
use ignit::wgpu;
use ignit::winit::event::MouseButton;

use crate::draw::DrawList;
use crate::layout::{NodeKind, PlacedNode, Rect, UiTree, hit_test};
use crate::render::GpuUiRenderer;
use crate::text::TextEngine;

pub struct SpikeModule;

impl Module for SpikeModule {
    fn setup(
        &self,
        _ecs: &mut World,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
    ) -> Vec<Box<dyn ignit::ecs::System>> {
        Vec::new()
    }

    fn register_overlays(
        &self,
        ecs: &mut World,
        layer_renderers: &mut Vec<Box<dyn LayerRenderer>>,
    ) {
        let Some(device) = ecs.get_resource::<DeviceResource>().map(|r| r.0.clone()) else {
            return;
        };
        let Some(queue) = ecs.get_resource::<QueueResource>().map(|r| r.0.clone()) else {
            return;
        };
        let format = ecs
            .get_resource::<SurfaceFormatResource>()
            .map(|resource| resource.0)
            .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);

        layer_renderers.push(Box::new(SpikeOverlay::new(&device, &queue, format)));
    }
}

struct SpikeOverlay {
    text: TextEngine,
    gpu: GpuUiRenderer,
    tree: Option<UiTree>,
    tree_key: (u32, u32, u32),
    button_on: bool,
    button_down: bool,
    hover: Option<&'static str>,
    draw: DrawList,
    frames: u64,
    last_report: Instant,
}

impl SpikeOverlay {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self {
            text: TextEngine::new(),
            gpu: GpuUiRenderer::new(device, queue, format),
            tree: None,
            tree_key: (0, 0, 0),
            button_on: false,
            button_down: false,
            hover: None,
            draw: DrawList::default(),
            frames: 0,
            last_report: Instant::now(),
        }
    }

    fn status(&self) -> &'static str {
        if self.button_on {
            "state: on"
        } else {
            "state: off"
        }
    }

    fn pointer(&self, ecs: &World) -> Option<(f32, f32)> {
        ecs.get_resource::<CursorPosition>()
            .map(|position| (position.0.x as f32, position.0.y as f32))
    }

    fn mouse_down(&self, ecs: &World) -> bool {
        ecs.get_resource::<InputSnapshot>()
            .and_then(|snapshot| {
                snapshot
                    .0
                    .button_state_any(&InputButton::Mouse(MouseButton::Left))
            })
            .is_some_and(|(_, pressed)| pressed)
    }

    fn build_draw(&mut self, nodes: &[PlacedNode], screen: (f32, f32)) {
        self.draw.clear();
        let full = Rect {
            x: 0.0,
            y: 0.0,
            w: screen.0,
            h: screen.1,
        };

        for node in nodes {
            let clip = node.clip.unwrap_or(full);
            match &node.kind {
                NodeKind::Root => {
                    self.draw
                        .rect(node.rect, [0.055, 0.065, 0.095, 1.0], 0.0, clip);
                }
                NodeKind::Panel { fill, radius } => {
                    let shadow = Rect {
                        x: node.rect.x + 4.0,
                        y: node.rect.y + 6.0,
                        ..node.rect
                    };
                    self.draw
                        .shadow(shadow, [0.0, 0.0, 0.0, 0.28], *radius, clip);
                    self.draw.rect(node.rect, *fill, *radius, clip);
                }
                NodeKind::Scroll => {
                    self.draw
                        .rect(node.rect, [0.075, 0.085, 0.12, 1.0], 8.0, clip);
                    self.draw.line(
                        Rect {
                            x: node.rect.x,
                            y: node.rect.y,
                            w: node.rect.w,
                            h: 1.0,
                        },
                        [0.22, 0.25, 0.34, 1.0],
                        clip,
                    );
                }
                NodeKind::Button { label, on } => {
                    let hovered = self.hover == Some(node.id);
                    let fill = if hovered {
                        if *on {
                            [0.20, 0.58, 0.42, 1.0]
                        } else {
                            [0.24, 0.31, 0.46, 1.0]
                        }
                    } else if *on {
                        [0.16, 0.48, 0.35, 1.0]
                    } else {
                        [0.17, 0.22, 0.33, 1.0]
                    };
                    self.draw.rect(node.rect, fill, 8.0, clip);
                    let label_size = 14.0;
                    let (label_w, label_h) = self.text.measure(label, label_size, node.rect.w);
                    let origin = [
                        node.rect.x + (node.rect.w - label_w) * 0.5,
                        node.rect.y + (node.rect.h - label_h) * 0.5,
                    ];
                    for quad in self
                        .text
                        .shape(label, label_size, node.rect.w, origin, [0.95; 4])
                    {
                        self.draw.glyph(
                            Rect {
                                x: quad.rect[0],
                                y: quad.rect[1],
                                w: quad.rect[2],
                                h: quad.rect[3],
                            },
                            quad.uv,
                            quad.color,
                            clip,
                        );
                    }
                }
                NodeKind::Image => {
                    self.draw
                        .image(node.rect, [1.0; 4], [0.0, 0.0, 1.0, 1.0], clip);
                }
                NodeKind::Label { text, size, color } => {
                    for quad in self.text.shape(
                        text,
                        *size,
                        node.content.w,
                        [node.content.x, node.content.y],
                        *color,
                    ) {
                        self.draw.glyph(
                            Rect {
                                x: quad.rect[0],
                                y: quad.rect[1],
                                w: quad.rect[2],
                                h: quad.rect[3],
                            },
                            quad.uv,
                            quad.color,
                            clip,
                        );
                    }
                }
                NodeKind::Spacer => {}
            }
        }
    }
}

impl LayerRenderer for SpikeOverlay {
    fn layer(&self) -> RenderLayer {
        RenderLayer::UI
    }

    fn debug_name(&self) -> &str {
        "ui-spike"
    }

    fn render(&mut self, ctx: ignit::app::RenderOverlayContext) {
        let (width, height) = ctx.screen_size;
        if width <= 0.0 || height <= 0.0 {
            return;
        }

        let scale = ctx
            .ecs
            .get_resource::<IcedEventQueue>()
            .map(|queue| queue.scale_factor as f32)
            .unwrap_or(1.0)
            .clamp(0.5, 3.0);
        let key = (
            width.round() as u32,
            height.round() as u32,
            (scale * 100.0).round() as u32,
        );
        if self.tree_key != key || self.tree.is_none() {
            self.tree = Some(UiTree::demo(width, height, scale, self.status()));
            self.tree_key = key;
        }

        let time = ctx
            .ecs
            .get_resource::<TotalTime>()
            .map(|time| time.0)
            .unwrap_or(0.0);
        let scroll = ((time * 0.9).sin() * 0.5 + 0.5) as f32;
        let scroll_offset = scroll * 180.0;

        let nodes =
            self.tree
                .as_mut()
                .unwrap()
                .compute(&mut self.text, width, height, scroll_offset);

        let pointer = self.pointer(ctx.ecs);
        self.hover = pointer.and_then(|(x, y)| hit_test(&nodes, x, y).map(|node| node.id));

        let down = self.mouse_down(ctx.ecs);
        if down
            && !self.button_down
            && let Some((x, y)) = pointer
            && let Some(hit) = hit_test(&nodes, x, y)
            && hit.id == "toggle"
        {
            self.button_on = !self.button_on;
            self.tree = Some(UiTree::demo(width, height, scale, self.status()));
            self.tree_key = key;
        }
        self.button_down = down;

        // Recompute after a click so the status text and button state are
        // reflected in the same frame.
        let nodes =
            self.tree
                .as_mut()
                .unwrap()
                .compute(&mut self.text, width, height, scroll_offset);
        self.build_draw(&nodes, (width, height));

        self.gpu.upload_atlas(ctx.queue, &mut self.text.atlas);
        if let Some(encoder) = ctx.encoder {
            self.gpu.render(
                ctx.device,
                ctx.queue,
                encoder,
                ctx.target_view,
                &self.draw,
                (width, height),
            );
        }

        self.frames += 1;
        if self.last_report.elapsed().as_secs_f32() > 2.0 {
            let (rasterized, hits, measure_hits, measure_misses) = self.text.stats();
            log::info!(
                "ui-spike: {} frames, {} verts, {} batches, glyphs rasterized={} cache_hits={} measure_hits={} measure_misses={}",
                self.frames,
                self.draw.vertices.len(),
                self.draw.batches.len(),
                rasterized,
                hits,
                measure_hits,
                measure_misses
            );
            self.last_report = Instant::now();
        }
    }
}
