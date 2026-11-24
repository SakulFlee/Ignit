//! Module wiring for the UI renderer.

use std::sync::{Arc, Mutex};

use ignit_app::{LayerRenderer, Module};
use ignit_ecs::{System, World};
use ignit_ecs_bridge::{DeviceResource, QueueResource, SurfaceFormatResource};
use ignit_ui_text::TextEngine;
use wgpu::{Device, Queue, TextureFormat};

use crate::UiLayerRenderer;

/// Shared text engine resource used by the runtime and the renderer.
#[derive(Clone)]
pub struct UiTextResource(pub Arc<Mutex<TextEngine>>);

/// Adds the UI layer renderer and publishes the shared text engine.
pub struct UiRenderModule {
    text: Arc<Mutex<TextEngine>>,
    format: Option<TextureFormat>,
}

impl UiRenderModule {
    pub fn new(text: Arc<Mutex<TextEngine>>) -> Self {
        Self { text, format: None }
    }

    pub fn with_format(mut self, format: TextureFormat) -> Self {
        self.format = Some(format);
        self
    }
}

impl Module for UiRenderModule {
    fn setup(&self, ecs: &mut World, _device: &Device, _queue: &Queue) -> Vec<Box<dyn System>> {
        ecs.insert_resource(UiTextResource(Arc::clone(&self.text)));
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
        let format = self
            .format
            .or_else(|| ecs.get_resource::<SurfaceFormatResource>().map(|r| r.0))
            .unwrap_or(TextureFormat::Bgra8UnormSrgb);
        layer_renderers.push(Box::new(UiLayerRenderer::new(
            &device,
            &queue,
            format,
            Arc::clone(&self.text),
        )));
    }
}
