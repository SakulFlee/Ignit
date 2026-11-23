//! # Ignit UI text
//!
//! Upstream `cosmic-text 0.19` shaping plus a multi-page R8 alpha glyph atlas.
//! The engine implements [`ignit_ui::UiTextMeasure`] for Taffy layout and
//! exposes atlas-backed quads for `ignit_ui_render`.
//!
//! The text edit model is display-independent; clipboard access is abstracted
//! through [`UiClipboard`] so headless tests do not need a window server.

pub mod atlas;
pub mod edit;
pub mod font;
pub mod shape;

pub use atlas::{AtlasGlyph, GlyphAtlas, GlyphPage};
pub use edit::{SystemClipboard, TextEditState, UiClipboard};
pub use font::FontStack;
pub use shape::{ShapedGlyph, ShapedLine, ShapedText, TextEngine, TextQuad, TextStats};
