//! # Ignit Engine
//!
//! A multi-platform 3D rendering engine built in Rust using wgpu as the graphics backend.

pub use ignit_2d as twod;
pub use ignit_animation as animation;
pub use ignit_app as app;
pub use ignit_app::{TouchControlsConfig, set_touch_controls, touch_controls};
pub use ignit_billboard as billboard;
pub use ignit_camera as camera;
pub use ignit_debug_render as debug_render;
pub use ignit_debug_ui as debug_ui;
pub use ignit_ecs as ecs;
pub use ignit_ecs_bridge as ecs_bridge;
pub use ignit_file_manager as file_manager;
pub use ignit_iced as iced;
pub use ignit_importer_gltf as importer;
pub use ignit_procgeo as procgeo;
pub use ignit_renderer as renderer;
pub use ignit_resources as resources;
pub use ignit_shader_compute as shader_compute;
pub use ignit_shader_preprocessor as shader_preprocessor;
pub use ignit_text as text;
pub use ignit_touch_ui as touch_ui;
pub use ignit_ui as ui;
pub use ignit_ui_render as ui_render;
pub use ignit_ui_text as ui_text;

#[cfg(test)]
pub mod wgpu_test_adapter;

pub use ignit_core::{cache, logging, mip_level, or, quaternion};

// Bootstrapping: `#[ignit::main]` on an app's entrypoint generates the
// desktop, Android, and iOS entrypoints.
pub use ignit_macros::main;

// Re-exports
pub use cgmath;
#[cfg(feature = "gamepad_gilrs")]
pub use gilrs;
pub use wgpu;
pub use winit;
