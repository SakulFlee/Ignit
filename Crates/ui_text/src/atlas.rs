//! Shelf-packed multi-page R8 alpha atlas with LRU page eviction.

use std::collections::HashMap;

use cosmic_text::{CacheKey, SwashContent, SwashImage};

#[derive(Clone, Copy, Debug)]
pub struct AtlasGlyph {
    pub page: usize,
    pub uv: [f32; 4],
    pub placement: [i32; 4],
}

struct Shelf {
    x: u32,
    y: u32,
    row_h: u32,
}

pub struct GlyphPage {
    pub size: u32,
    pub pixels: Vec<u8>,
    pub dirty: bool,
    shelf: Shelf,
    pub last_used: u64,
}

impl GlyphPage {
    fn new(size: u32) -> Self {
        Self {
            size,
            pixels: vec![0; (size * size) as usize],
            dirty: true,
            shelf: Shelf {
                x: 0,
                y: 0,
                row_h: 0,
            },
            last_used: 0,
        }
    }

    fn clear(&mut self) {
        self.pixels.fill(0);
        self.shelf = Shelf {
            x: 0,
            y: 0,
            row_h: 0,
        };
        self.dirty = true;
    }
}

pub struct GlyphAtlas {
    pub pages: Vec<GlyphPage>,
    location: HashMap<CacheKey, AtlasGlyph>,
    page_size: u32,
    max_pages: usize,
    frame: u64,
    pub rasterizations: u64,
    pub cache_hits: u64,
    pub evictions: u64,
}

impl GlyphAtlas {
    pub fn new(page_size: u32, max_pages: usize) -> Self {
        Self {
            pages: vec![GlyphPage::new(page_size)],
            location: HashMap::new(),
            page_size,
            max_pages: max_pages.max(1),
            frame: 0,
            rasterizations: 0,
            cache_hits: 0,
            evictions: 0,
        }
    }

    pub fn page_size(&self) -> u32 {
        self.page_size
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    pub fn get(&mut self, key: CacheKey) -> Option<AtlasGlyph> {
        let glyph = *self.location.get(&key)?;
        if let Some(page) = self.pages.get_mut(glyph.page) {
            page.last_used = self.frame;
        }
        self.cache_hits += 1;
        Some(glyph)
    }

    /// Ensures a glyph is resident and returns its atlas coordinates.
    pub fn insert(&mut self, key: CacheKey, image: &SwashImage) -> Option<AtlasGlyph> {
        if let Some(glyph) = self.get(key) {
            return Some(glyph);
        }
        if image.content != SwashContent::Mask {
            return None;
        }
        let width = image.placement.width;
        let height = image.placement.height;
        if width == 0 || height == 0 {
            return None;
        }

        let pad = 1;
        let mut page_index = self
            .pages
            .iter()
            .position(|page| {
                page.shelf.x + width + pad <= page.size && page.shelf.y + height + pad <= page.size
            })
            .unwrap_or_else(|| {
                if self.pages.len() < self.max_pages {
                    self.pages.push(GlyphPage::new(self.page_size));
                    self.pages.len() - 1
                } else {
                    self.evict_lru_page()
                }
            });

        let page = &mut self.pages[page_index];
        if page.shelf.x + width + pad > page.size {
            page.shelf.x = 0;
            page.shelf.y += page.shelf.row_h + pad;
            page.shelf.row_h = 0;
        }
        if page.shelf.y + height + pad > page.size {
            // The chosen page cannot fit the glyph; clear the least recently
            // used page and try again from its top-left corner.
            page_index = self.evict_lru_page();
            self.pages[page_index].clear();
        }

        let page = &mut self.pages[page_index];
        let x = page.shelf.x;
        let y = page.shelf.y;
        for row in 0..height {
            let src = (row * width) as usize;
            let dst = ((y + row) * page.size + x) as usize;
            page.pixels[dst..dst + width as usize]
                .copy_from_slice(&image.data[src..src + width as usize]);
        }
        page.shelf.x += width + pad;
        page.shelf.row_h = page.shelf.row_h.max(height);
        page.dirty = true;
        page.last_used = self.frame;

        let half_texel = 0.5 / page.size as f32;
        let glyph = AtlasGlyph {
            page: page_index,
            uv: [
                x as f32 / page.size as f32 + half_texel,
                y as f32 / page.size as f32 + half_texel,
                (x + width) as f32 / page.size as f32 - half_texel,
                (y + height) as f32 / page.size as f32 - half_texel,
            ],
            placement: [
                image.placement.left,
                image.placement.top,
                width as i32,
                height as i32,
            ],
        };
        self.location.insert(key, glyph);
        self.rasterizations += 1;
        Some(glyph)
    }

    fn evict_lru_page(&mut self) -> usize {
        let index = self
            .pages
            .iter()
            .enumerate()
            .min_by_key(|(_, page)| page.last_used)
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.location.retain(|_, glyph| glyph.page != index);
        self.pages[index].clear();
        self.evictions += 1;
        index
    }

    /// Returns page indices whose pixels changed since the last call.
    pub fn take_dirty_pages(&mut self) -> Vec<usize> {
        let mut dirty = Vec::new();
        for (index, page) in self.pages.iter_mut().enumerate() {
            if page.dirty {
                page.dirty = false;
                dirty.push(index);
            }
        }
        dirty
    }
}
