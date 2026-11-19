//! # Resources Module (Re-export Shim)
//!
//! This crate re-exports all resource types from their new fine-grained
//! crates. It exists purely for backward compatibility — new code should
//! import directly from the specific crate.

// Flat re-exports for backward compatibility
pub use ignit_animation::*;
pub use ignit_camera::*;
pub use ignit_cull::*;
pub use ignit_ibl_brdf::*;
pub use ignit_instance::*;
pub use ignit_light::*;
pub use ignit_material_shader::*;
pub use ignit_math::{Mode, Transform, ortho_wgpu, perspective_wgpu};
pub use ignit_mesh::*;
pub use ignit_model::*;
pub use ignit_shader_compute::*;
pub use ignit_shader_core::*;
pub use ignit_shadow::*;
pub use ignit_texture::*;
pub use ignit_vertex::*;
pub use ignit_world_environment::*;

// Re-export PBR material shader types
pub use ignit_shader_pbr::*;
