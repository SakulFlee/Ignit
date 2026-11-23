//! Minimal cosmic-text shaping and R8 alpha-atlas proof for the UI spike.

use std::collections::HashMap;

use cosmic_text::{
    Attrs, Buffer, CacheKey, FontSystem, Metrics, Shaping, SwashCache, SwashContent, SwashImage,
};
use ignit::text::DefaultFont;

/// A glyph copied into the alpha atlas.
#[derive(Clone, Copy, Debug)]
pub struct AtlasGlyph {
    pub uv: [f32; 4],
    pub placement: [i32; 4],
}

/// A single textured glyph quad in logical/physical pixels.
#[derive(Clone, Copy, Debug)]
pub struct TextQuad {
    pub rect: [f32; 4],
    pub uv: [f32; 4],
    pub color: [f32; 4],
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct MeasureKey {
    text: String,
    size_bits: u32,
    width_bits: u32,
}

struct Shelf {
    x: u32,
    y: u32,
    row_h: u32,
}

/// Shelf-packed single-page R8 atlas.
pub struct GlyphAtlas {
    pub size: u32,
    pub pixels: Vec<u8>,
    pub glyphs: HashMap<CacheKey, AtlasGlyph>,
    shelf: Shelf,
    pub dirty: bool,
    pub rasterizations: u64,
    pub cache_hits: u64,
}

impl GlyphAtlas {
    pub fn new(size: u32) -> Self {
        Self {
            size,
            pixels: vec![0; (size * size) as usize],
            glyphs: HashMap::new(),
            shelf: Shelf {
                x: 0,
                y: 0,
                row_h: 0,
            },
            dirty: true,
            rasterizations: 0,
            cache_hits: 0,
        }
    }

    fn insert(&mut self, key: CacheKey, image: &SwashImage) -> Option<AtlasGlyph> {
        if let Some(glyph) = self.glyphs.get(&key) {
            self.cache_hits += 1;
            return Some(*glyph);
        }

        if image.content != SwashContent::Mask {
            return None;
        }

        let w = image.placement.width;
        let h = image.placement.height;
        if w == 0 || h == 0 {
            return None;
        }

        let pad = 1;
        if self.shelf.x + w + pad > self.size {
            self.shelf.x = 0;
            self.shelf.y += self.shelf.row_h + pad;
            self.shelf.row_h = 0;
        }
        if self.shelf.y + h + pad > self.size {
            // The spike intentionally keeps one page. A real atlas grows to
            // more pages instead of evicting live glyphs.
            log::warn!("ui-spike: alpha atlas full, resetting");
            self.pixels.fill(0);
            self.glyphs.clear();
            self.shelf = Shelf {
                x: 0,
                y: 0,
                row_h: 0,
            };
        }

        let x = self.shelf.x;
        let y = self.shelf.y;
        for row in 0..h {
            let src = (row * w) as usize;
            let dst = ((y + row) * self.size + x) as usize;
            self.pixels[dst..dst + w as usize].copy_from_slice(&image.data[src..src + w as usize]);
        }

        self.shelf.x += w + pad;
        self.shelf.row_h = self.shelf.row_h.max(h);
        self.dirty = true;
        self.rasterizations += 1;

        let glyph = AtlasGlyph {
            uv: [
                x as f32 / self.size as f32,
                y as f32 / self.size as f32,
                (x + w) as f32 / self.size as f32,
                (y + h) as f32 / self.size as f32,
            ],
            placement: [
                image.placement.left,
                image.placement.top,
                w as i32,
                h as i32,
            ],
        };
        self.glyphs.insert(key, glyph);
        Some(glyph)
    }
}

pub struct TextEngine {
    font_system: FontSystem,
    swash_cache: SwashCache,
    pub atlas: GlyphAtlas,
    measure_cache: HashMap<MeasureKey, (f32, f32)>,
    pub measure_hits: u64,
    pub measure_misses: u64,
    family: String,
}

impl TextEngine {
    pub fn new() -> Self {
        let mut font_system = FontSystem::new_with_fonts([]);
        font_system
            .db_mut()
            .load_font_data(DefaultFont::bytes().to_vec());
        let family = font_system
            .db()
            .faces()
            .next()
            .and_then(|face| face.families.first().map(|(name, _)| name.clone()))
            .unwrap_or_else(|| "Inter".to_string());
        font_system.db_mut().set_sans_serif_family(family.clone());

        Self {
            font_system,
            swash_cache: SwashCache::new(),
            atlas: GlyphAtlas::new(1024),
            measure_cache: HashMap::new(),
            measure_hits: 0,
            measure_misses: 0,
            family,
        }
    }

    pub fn family(&self) -> &str {
        &self.family
    }

    fn attrs(&self) -> Attrs<'_> {
        Attrs::new()
    }

    /// Measures wrapped text. The width is a hard wrap width in pixels.
    pub fn measure(&mut self, text: &str, size: f32, width: f32) -> (f32, f32) {
        let width = width.max(1.0);
        let key = MeasureKey {
            text: text.to_string(),
            size_bits: size.to_bits(),
            width_bits: width.to_bits(),
        };
        if let Some(value) = self.measure_cache.get(&key) {
            self.measure_hits += 1;
            return *value;
        }
        self.measure_misses += 1;

        let mut buffer = Buffer::new(&mut self.font_system, Metrics::new(size, size * 1.25));
        buffer.set_size(Some(width), None);
        let attrs = self.attrs();
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let mut measured_w: f32 = 0.0;
        let mut measured_h: f32 = 0.0;
        for run in buffer.layout_runs() {
            measured_w = measured_w.max(run.line_w);
            measured_h += run.line_height;
        }

        let value = (measured_w.ceil(), measured_h.ceil().max(size));
        self.measure_cache.insert(key, value);
        value
    }

    /// Shapes text at its final width and returns glyph quads.
    pub fn shape(
        &mut self,
        text: &str,
        size: f32,
        width: f32,
        origin: [f32; 2],
        color: [f32; 4],
    ) -> Vec<TextQuad> {
        let mut buffer = Buffer::new(&mut self.font_system, Metrics::new(size, size * 1.25));
        buffer.set_size(Some(width.max(1.0)), None);
        let attrs = self.attrs();
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let mut quads = Vec::new();
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                let physical = glyph.physical((0.0, run.line_y), 1.0);
                let image = self
                    .swash_cache
                    .get_image(&mut self.font_system, physical.cache_key);
                let Some(image) = image else {
                    continue;
                };
                let Some(glyph_atlas) = self.atlas.insert(physical.cache_key, image) else {
                    continue;
                };
                if glyph_atlas.placement[2] == 0 || glyph_atlas.placement[3] == 0 {
                    continue;
                }

                let x = origin[0] + physical.x as f32 + glyph_atlas.placement[0] as f32;
                let y = origin[1] + physical.y as f32 - glyph_atlas.placement[1] as f32;
                quads.push(TextQuad {
                    rect: [
                        x,
                        y,
                        glyph_atlas.placement[2] as f32,
                        glyph_atlas.placement[3] as f32,
                    ],
                    uv: glyph_atlas.uv,
                    color,
                });
            }
        }
        quads
    }

    pub fn stats(&self) -> (u64, u64, u64, u64) {
        (
            self.atlas.rasterizations,
            self.atlas.cache_hits,
            self.measure_hits,
            self.measure_misses,
        )
    }
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new()
    }
}
