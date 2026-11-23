//! Ignit UI renderer: batched quads, glyphs, textures, and scissor clipping.

mod module;
mod renderer;

pub use module::{UiRenderModule, UiTextResource};
pub use renderer::{UiBatch, UiLayerRenderer, UiRenderStats, UiRenderer, UiTextureRef, UiVertex};
