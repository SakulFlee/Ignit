//! # Ignit ECS Bridge
//!
//! Bridge types that connect the ECS library (`ignit_ecs`) with engine-specific
//! state — frame timing, input, window, and GPU resources. These types are designed
//! to be inserted into `ignit_ecs::World` as **resources** via
//! `world.insert_resource(...)`, making them accessible to ECS systems through
//! `Res<T>` / `ResMut<T>` parameters.
//!
//! This crate exists to keep `ignit_ecs` engine-agnostic while providing
//! the glue layer that Ignit's runtime, renderer, and game code depend on.

extern crate self as ignit_ecs_bridge;

pub mod resources;
pub use resources::*;

pub mod components;
pub use components::*;

pub mod editable;
pub mod fields;
pub use editable::*;
pub use fields::*;

/// Derive the per-field editing metadata for a component.
///
/// Named fields whose types implement [`EditableValue`] are included. Mark
/// unsupported fields with `#[editable(skip)]`.
pub use ignit_macros::EditableFields;
