//! cosmic-text shaping, cached layout, and atlas-backed glyph quads.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use cosmic_text::{
    Attrs, Buffer, CacheKey, Ellipsize, EllipsizeHeightLimit, FontSystem, Metrics, Shaping,
    SwashCache, Wrap,
};
use ignit_ui::{UiColor, UiSize, UiTextMeasure, UiTextStyle, UiTextWrap};

use crate::atlas::GlyphAtlas;
use crate::font::{FontStack, build_font_system, family_for};

#[derive(Clone, Debug)]
pub struct ShapedGlyph {
    pub cache_key: CacheKey,
    pub rect: [f32; 4],
    pub color: [f32; 4],
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug)]
pub struct ShapedLine {
    pub y: f32,
    pub height: f32,
    pub width: f32,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Default)]
pub struct ShapedText {
    pub glyphs: Vec<ShapedGlyph>,
    pub lines: Vec<ShapedLine>,
    pub width: f32,
    pub height: f32,
}

impl ShapedText {
    pub fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TextQuad {
    pub rect: [f32; 4],
    pub uv: [f32; 4],
    pub color: [f32; 4],
    pub page: usize,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct TextKey {
    text: String,
    size_bits: u32,
    line_height_bits: u32,
    family: Option<String>,
    weight: u16,
    wrap: u8,
    ellipsis: bool,
    width_bits: u32,
    scale_bits: u32,
}

pub struct TextEngine {
    font_system: FontSystem,
    swash_cache: SwashCache,
    pub atlas: GlyphAtlas,
    stack: FontStack,
    default_family: String,
    scale: f32,
    cache: HashMap<TextKey, Arc<ShapedText>>,
    order: VecDeque<TextKey>,
    max_cached: usize,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

impl TextEngine {
    pub fn new() -> Self {
        Self::with_font_stack(FontStack::default(), false)
    }

    pub fn with_system_fonts() -> Self {
        Self::with_font_stack(FontStack::default(), true)
    }

    pub fn with_font_stack(stack: FontStack, system_fonts: bool) -> Self {
        let setup = build_font_system(&stack, system_fonts);
        Self {
            font_system: setup.system,
            swash_cache: SwashCache::new(),
            atlas: GlyphAtlas::new(1024, 4),
            stack,
            default_family: setup.default_family,
            scale: 1.0,
            cache: HashMap::new(),
            order: VecDeque::new(),
            max_cached: 512,
            cache_hits: 0,
            cache_misses: 0,
        }
    }

    pub fn with_atlas(stack: FontStack, page_size: u32, max_pages: usize) -> Self {
        let setup = build_font_system(&stack, false);
        Self {
            font_system: setup.system,
            swash_cache: SwashCache::new(),
            atlas: GlyphAtlas::new(page_size, max_pages),
            stack,
            default_family: setup.default_family,
            scale: 1.0,
            cache: HashMap::new(),
            order: VecDeque::new(),
            max_cached: 512,
            cache_hits: 0,
            cache_misses: 0,
        }
    }

    pub fn default_family(&self) -> &str {
        &self.default_family
    }

    pub fn font_stack(&self) -> &FontStack {
        &self.stack
    }

    pub fn set_scale(&mut self, scale: f32) {
        let scale = scale.clamp(0.5, 4.0);
        if (self.scale - scale).abs() > f32::EPSILON {
            self.scale = scale;
            self.cache.clear();
            self.order.clear();
        }
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn begin_frame(&mut self) {
        self.atlas.begin_frame();
    }

    pub fn shape(&mut self, text: &str, style: &UiTextStyle, max_width: f32) -> Arc<ShapedText> {
        let width = max_width.max(1.0);
        let key = TextKey {
            text: text.to_string(),
            size_bits: style.size.to_bits(),
            line_height_bits: style.line_height.unwrap_or(style.size * 1.25).to_bits(),
            family: style.family.clone(),
            weight: style.weight,
            wrap: wrap_id(style.wrap),
            ellipsis: style.ellipsis,
            width_bits: width.to_bits(),
            scale_bits: self.scale.to_bits(),
        };
        if let Some(shaped) = self.cache.get(&key) {
            self.cache_hits += 1;
            return Arc::clone(shaped);
        }
        self.cache_misses += 1;

        let shaped = Arc::new(self.shape_uncached(text, style, width));
        if self.cache.len() >= self.max_cached
            && let Some(oldest) = self.order.pop_front()
        {
            self.cache.remove(&oldest);
        }
        self.order.push_back(key.clone());
        self.cache.insert(key, Arc::clone(&shaped));
        shaped
    }

    fn shape_uncached(&mut self, text: &str, style: &UiTextStyle, width: f32) -> ShapedText {
        let line_height = style.line_height.unwrap_or(style.size * 1.25);
        let mut buffer = Buffer::new(&mut self.font_system, Metrics::new(style.size, line_height));
        buffer.set_size(Some(width), None);
        buffer.set_wrap(match style.wrap {
            UiTextWrap::NoWrap => Wrap::None,
            UiTextWrap::Word => Wrap::Word,
            UiTextWrap::Glyph => Wrap::Glyph,
        });
        if style.ellipsis {
            buffer.set_ellipsize(Ellipsize::End(EllipsizeHeightLimit::Height(line_height)));
        }
        let family = family_for(&self.stack, style);
        let attrs = Attrs::new()
            .family(family)
            .weight(cosmic_text::Weight(style.weight));
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let color = resolve_color(&style.color);
        let scale = self.scale.max(0.5);
        let mut shaped = ShapedText::default();
        for run in buffer.layout_runs() {
            let mut line_start = usize::MAX;
            let mut line_end = 0usize;
            for glyph in run.glyphs {
                let physical = glyph.physical((0.0, run.line_y), scale);
                let Some(image) = self
                    .swash_cache
                    .get_image(&mut self.font_system, physical.cache_key)
                else {
                    continue;
                };
                if image.placement.width == 0 || image.placement.height == 0 {
                    continue;
                }
                let x = physical.x as f32 / scale + image.placement.left as f32 / scale;
                let y = physical.y as f32 / scale - image.placement.top as f32 / scale;
                let w = image.placement.width as f32 / scale;
                let h = image.placement.height as f32 / scale;
                shaped.glyphs.push(ShapedGlyph {
                    cache_key: physical.cache_key,
                    rect: [x, y, w, h],
                    color,
                    start: glyph.start,
                    end: glyph.end,
                });
                line_start = line_start.min(glyph.start);
                line_end = line_end.max(glyph.end);
                shaped.width = shaped.width.max(glyph.x + glyph.w);
            }
            if line_start != usize::MAX {
                shaped.lines.push(ShapedLine {
                    y: run.line_top,
                    height: run.line_height,
                    width: run.line_w,
                    start: line_start,
                    end: line_end,
                });
                shaped.height += run.line_height;
            }
        }
        shaped
    }

    /// Resolves atlas UVs for a shaped run. Call once per frame before recording.
    pub fn quads(&mut self, shaped: &ShapedText) -> Vec<TextQuad> {
        let mut quads = Vec::with_capacity(shaped.glyphs.len());
        for glyph in &shaped.glyphs {
            let Some(image) = self
                .swash_cache
                .get_image(&mut self.font_system, glyph.cache_key)
            else {
                continue;
            };
            let Some(atlas_glyph) = self.atlas.insert(glyph.cache_key, image) else {
                continue;
            };
            quads.push(TextQuad {
                rect: glyph.rect,
                uv: atlas_glyph.uv,
                color: glyph.color,
                page: atlas_glyph.page,
            });
        }
        quads
    }

    pub fn hit_to_cursor(&mut self, shaped: &ShapedText, x: f32, y: f32) -> usize {
        let line = shaped
            .lines
            .iter()
            .find(|line| y >= line.y && y <= line.y + line.height)
            .or_else(|| shaped.lines.last());
        let Some(line) = line else {
            return 0;
        };
        let mut best = line.end;
        for glyph in shaped
            .glyphs
            .iter()
            .filter(|glyph| glyph.start >= line.start && glyph.end <= line.end)
        {
            if x < glyph.rect[0] {
                best = glyph.start;
                break;
            }
            if x <= glyph.rect[0] + glyph.rect[2] {
                best = if x < glyph.rect[0] + glyph.rect[2] * 0.5 {
                    glyph.start
                } else {
                    glyph.end
                };
                break;
            }
            best = glyph.end;
        }
        best
    }

    pub fn cursor_position(&mut self, shaped: &ShapedText, cursor: usize) -> Option<(f32, f32)> {
        let glyph = shaped
            .glyphs
            .iter()
            .find(|glyph| cursor >= glyph.start && cursor <= glyph.end)
            .or_else(|| shaped.glyphs.last())?;
        let line = shaped
            .lines
            .iter()
            .find(|line| cursor >= line.start && cursor <= line.end)?;
        Some((glyph.rect[0], line.y))
    }

    pub fn stats(&self) -> TextStats {
        TextStats {
            cache_hits: self.cache_hits,
            cache_misses: self.cache_misses,
            rasterizations: self.atlas.rasterizations,
            atlas_hits: self.atlas.cache_hits,
            atlas_pages: self.atlas.page_count(),
            evictions: self.atlas.evictions,
        }
    }
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl UiTextMeasure for TextEngine {
    fn measure(&mut self, text: &str, style: &UiTextStyle, max_width: Option<f32>) -> UiSize {
        let width = max_width.unwrap_or(100_000.0);
        let shaped = self.shape(text, style, width);
        UiSize::new(shaped.width.ceil(), shaped.height.ceil())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TextStats {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub rasterizations: u64,
    pub atlas_hits: u64,
    pub atlas_pages: usize,
    pub evictions: u64,
}

fn wrap_id(wrap: UiTextWrap) -> u8 {
    match wrap {
        UiTextWrap::NoWrap => 0,
        UiTextWrap::Word => 1,
        UiTextWrap::Glyph => 2,
    }
}

fn resolve_color(color: &UiColor) -> [f32; 4] {
    match color {
        UiColor::Rgba(color) => *color,
        UiColor::Token(_) => [1.0, 1.0, 1.0, 1.0],
    }
}
