//! Ignit UI renderer: batched quads, glyphs, textures, and scissor clipping.

mod renderer;
mod runtime;

pub use renderer::{UiBatch, UiLayerRenderer, UiRenderStats, UiRenderer, UiTextureRef, UiVertex};
pub use runtime::{UiRenderModule, UiTextResource};
