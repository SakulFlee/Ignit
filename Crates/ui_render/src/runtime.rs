//! Generic window-event conversion and the UI update system.

use std::sync::{Arc, Mutex};

use ignit_app::{Module, UiSystem};
use ignit_ecs::{System, World};
use ignit_ecs_bridge::{
    CursorIcon, DeltaTime, DeviceResource, InputCapture, QueueResource, SurfaceFormatResource,
    WindowEvent, WindowEventQueue, WindowScaleFactor,
};
use ignit_ui::{
    UiInputCapture, UiInputState, UiKey, UiModifiers, UiNotifications, UiPoint, UiView,
};
use ignit_ui_text::TextEngine;
use wgpu::{Device, Queue, TextureFormat};
use winit::event::{ElementState, MouseScrollDelta, TouchPhase};
use winit::keyboard::{Key, NamedKey};

use crate::UiLayerRenderer;

#[derive(Clone)]
pub struct UiTextResource(pub Arc<Mutex<TextEngine>>);

/// Adds the UI layer renderer and the pre-game UI update phase.
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
        ignit_ui::init(ecs);
        ecs.insert_resource(UiTextResource(Arc::clone(&self.text)));
        if ecs.get_resource::<InputCapture>().is_none() {
            ecs.insert_resource(InputCapture::default());
        }
        Vec::new()
    }

    fn register_overlays(
        &self,
        ecs: &mut World,
        layer_renderers: &mut Vec<Box<dyn ignit_app::LayerRenderer>>,
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

    fn ui_systems(&self, _ecs: &mut World) -> Vec<Box<dyn UiSystem>> {
        vec![Box::new(UiUpdateSystem::new(Arc::clone(&self.text)))]
    }
}

pub(crate) struct UiUpdateSystem {
    text: Arc<Mutex<TextEngine>>,
    touch: Option<u64>,
}

impl UiUpdateSystem {
    pub(crate) fn new(text: Arc<Mutex<TextEngine>>) -> Self {
        Self { text, touch: None }
    }
}

impl UiSystem for UiUpdateSystem {
    fn name(&self) -> &str {
        "ignit-ui-update"
    }

    fn run(&mut self, world: &mut World) {
        apply_window_events(world, &mut self.touch);
        let delta = world
            .get_resource::<DeltaTime>()
            .map(|time| time.0)
            .unwrap_or(0.0);
        if let Some(mut notifications) = world.get_resource_mut::<UiNotifications>() {
            notifications.advance(delta);
        }
        let mut text = self.text.lock().expect("ui text mutex poisoned");
        ignit_ui::update(world, &mut *text);
        publish_capture(world, self.touch);
    }
}

fn apply_window_events(world: &mut World, touch: &mut Option<u64>) {
    let events = world
        .get_resource_mut::<WindowEventQueue>()
        .map(|mut queue| queue.drain())
        .unwrap_or_default();
    let mut scale = world
        .get_resource::<WindowScaleFactor>()
        .map(|scale| scale.0 as f32)
        .unwrap_or(1.0)
        .max(0.5);

    let mut resize = None;
    let mut input = world
        .get_resource::<UiInputState>()
        .map(|input| (*input).clone())
        .unwrap_or_default();

    for event in events {
        match event {
            WindowEvent::CursorMoved { position } => {
                input.pointer = UiPoint::new(position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput { state, button } => {
                if button == winit::event::MouseButton::Left {
                    match state {
                        ElementState::Pressed => {
                            input.primary_down = true;
                            input.primary_pressed = true;
                        }
                        ElementState::Released => {
                            input.primary_down = false;
                            input.primary_released = true;
                        }
                    }
                }
            }
            WindowEvent::MouseWheel { delta } => {
                let (x, y) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * 40.0, -y * 40.0),
                    MouseScrollDelta::PixelDelta(position) => {
                        (position.x as f32, -(position.y as f32))
                    }
                };
                input.wheel.x += x;
                input.wheel.y += y;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed
                    && let Some(key) = ui_key(&event.logical_key)
                {
                    input.keys_pressed.push(key);
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                input.modifiers = UiModifiers {
                    shift: modifiers.shift_key(),
                    ctrl: modifiers.control_key(),
                    alt: modifiers.alt_key(),
                    super_key: modifiers.super_key(),
                };
            }
            WindowEvent::Resized(size) => {
                resize = Some(size);
            }
            WindowEvent::ScaleFactorChanged(new_scale) => {
                scale = new_scale as f32;
                if let Some(mut factor) = world.get_resource_mut::<WindowScaleFactor>() {
                    factor.0 = new_scale;
                }
            }
            WindowEvent::Focused(focused) => {
                if !focused {
                    input.primary_down = false;
                    input.keys_pressed.clear();
                }
            }
            WindowEvent::Touch(touch_event) => {
                input.pointer =
                    UiPoint::new(touch_event.location.x as f32, touch_event.location.y as f32);
                match touch_event.phase {
                    TouchPhase::Started => {
                        input.primary_down = true;
                        input.primary_pressed = true;
                        *touch = Some(touch_event.id);
                    }
                    TouchPhase::Moved => {
                        input.primary_down = true;
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        input.primary_down = false;
                        input.primary_released = true;
                        touch.take();
                    }
                }
            }
            WindowEvent::Ime(winit::event::Ime::Commit(text)) => {
                input.text.push_str(&text);
            }
            WindowEvent::Ime(_) => {}
        }
    }

    world.insert_resource(input);
    if let Some(size) = resize {
        let logical = (size.width as f32 / scale, size.height as f32 / scale);
        let roots = world
            .get_resource::<ignit_ui::UiForest>()
            .map(|forest| forest.roots().to_vec())
            .unwrap_or_default();
        if let Some(views) = world.get_component_store_mut::<UiView>() {
            let store = views.get_mut_store();
            for root in roots {
                if let Some(view) = store.get_component_mut(root.index) {
                    view.size = ignit_ui::UiSize::new(logical.0, logical.1);
                    view.scale = scale;
                }
            }
        }
    }
}

fn publish_capture(world: &mut World, touch: Option<u64>) {
    let ui_capture = world
        .get_resource::<UiInputCapture>()
        .map(|capture| (*capture).clone())
        .unwrap_or_default();
    let input = world
        .get_resource::<UiInputState>()
        .map(|input| (*input).clone())
        .unwrap_or_default();
    let Some(mut capture) = world.get_resource_mut::<InputCapture>() else {
        return;
    };
    capture.pointer |= ui_capture.pointer;
    capture.wheel |= ui_capture.wheel;
    capture.keyboard |= ui_capture.keyboard;
    capture.text |= ui_capture.text;
    capture.cursor = ui_capture.cursor.map(|cursor| match cursor {
        ignit_ui::UiCursorIcon::Default => CursorIcon::Default,
        ignit_ui::UiCursorIcon::Pointer => CursorIcon::Pointer,
        ignit_ui::UiCursorIcon::Text => CursorIcon::Text,
        ignit_ui::UiCursorIcon::ResizeHorizontal => CursorIcon::ResizeHorizontal,
        ignit_ui::UiCursorIcon::ResizeVertical => CursorIcon::ResizeVertical,
        ignit_ui::UiCursorIcon::Grab => CursorIcon::Grab,
        ignit_ui::UiCursorIcon::Grabbing => CursorIcon::Grabbing,
    });
    if let Some(id) = touch
        && ui_capture.pointer
    {
        capture.captured_touches.insert(id);
    }
    if input.primary_down && capture.pointer {
        capture.mouse_drag = true;
    }
}

fn ui_key(key: &Key) -> Option<UiKey> {
    match key {
        Key::Named(NamedKey::Tab) => Some(UiKey::Tab),
        Key::Named(NamedKey::Enter) => Some(UiKey::Enter),
        Key::Named(NamedKey::Escape) => Some(UiKey::Escape),
        Key::Named(NamedKey::Backspace) => Some(UiKey::Backspace),
        Key::Named(NamedKey::Delete) => Some(UiKey::Delete),
        Key::Named(NamedKey::ArrowLeft) => Some(UiKey::ArrowLeft),
        Key::Named(NamedKey::ArrowRight) => Some(UiKey::ArrowRight),
        Key::Named(NamedKey::ArrowUp) => Some(UiKey::ArrowUp),
        Key::Named(NamedKey::ArrowDown) => Some(UiKey::ArrowDown),
        Key::Named(NamedKey::Home) => Some(UiKey::Home),
        Key::Named(NamedKey::End) => Some(UiKey::End),
        Key::Named(NamedKey::Space) => Some(UiKey::Space),
        Key::Character(text) => text.chars().next().map(UiKey::Character),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ignit_ecs_bridge::{CursorIcon, WindowEvent};

    #[test]
    fn window_events_update_pointer_and_buttons() {
        let mut world = World::new();
        ignit_ui::init(&mut world);
        world.insert_resource(WindowEventQueue::default());
        world.insert_resource(WindowScaleFactor(1.0));
        world.insert_resource(InputCapture::default());
        if let Some(mut queue) = world.get_resource_mut::<WindowEventQueue>() {
            queue.push(WindowEvent::CursorMoved {
                position: winit::dpi::PhysicalPosition::new(120.0, 80.0),
            });
            queue.push(WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: winit::event::MouseButton::Left,
            });
            queue.push(WindowEvent::MouseWheel {
                delta: MouseScrollDelta::LineDelta(0.0, 1.0),
            });
        }
        let mut touch = None;
        apply_window_events(&mut world, &mut touch);
        let input = world.get_resource::<UiInputState>().unwrap();
        assert_eq!(input.pointer, UiPoint::new(120.0, 80.0));
        assert!(input.primary_down);
        assert!(input.primary_pressed);
        assert_eq!(input.wheel.y, -40.0);
    }

    #[test]
    fn ui_capture_maps_to_generic_capture() {
        let mut world = World::new();
        ignit_ui::init(&mut world);
        world.insert_resource(InputCapture::default());
        if let Some(mut capture) = world.get_resource_mut::<UiInputCapture>() {
            capture.pointer = true;
            capture.keyboard = true;
            capture.text = true;
            capture.cursor = Some(ignit_ui::UiCursorIcon::Text);
        }
        publish_capture(&mut world, Some(7));
        let capture = world.get_resource::<InputCapture>().unwrap();
        assert!(capture.pointer);
        assert!(capture.keyboard);
        assert!(capture.text);
        assert_eq!(capture.cursor, Some(CursorIcon::Text));
        assert!(capture.captures_touch(7));
    }
}
