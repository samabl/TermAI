//! S7 glyph atlas: `AtlasKey` -> deterministic slot allocation in pages (kernel/03 section 3.4).
//!
//! The atlas is the second stage after shaping: S6 says *which* glyphs a row draws and into
//! which cells, S7 says *where* their bitmaps live. This slice owns the key, the page/slot
//! bookkeeping, the eviction rule and the three failure-localisation counters the spec's S7 row
//! asks for (`miss` count, eviction count, rebuild generation). It owns no rasteriser: no
//! bitmap is produced, no page is uploaded and no pixel exists until the S8/S9 slice binds
//! `swash`'s rasteriser and a GPU texture to these slots - which is also why this module can be
//! tested with no GPU and no window.
//!
//! Determinism: allocation is a pure function of the insertion sequence and the glyph sizes.
//! Pages are packed by a shelf allocator in page order, the index is a `HashMap` that is never
//! iterated for allocation (its iteration order must never leak into a slot), and the LRU is a
//! vector rather than a hash set. The same sequence therefore produces byte-identical slots.

use std::collections::HashMap;

use crate::shape::{AaMode, FontId};

/// The atlas primary key (kernel/03 section 3.4). Every field participates in the hash: a
/// different glyph id, font, device pixel size or AA preset is a different bitmap.
///
/// The spec's full key also carries `scale_q8`, `size_q6`, `weight`, `slant` and `synth`
/// (kernel/03 section 3.4 / K-12). Those arrive with the DPI, synthetic-style and
/// cross-display slices; this slice carries the four fields its consumers already have.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct AtlasKey {
    /// Logical font id (the fallback chain's hit result).
    pub font_id: FontId,
    /// Glyph id inside that font, not a codepoint (kernel/03 section 3.4 `glyph`).
    pub glyph_id: u32,
    /// Device pixel size.
    pub px_size: u16,
    /// Anti-aliasing preset (AR-14: both grayscale).
    pub aa_mode: AaMode,
}

/// The bitmap size a caller wants reserved for one glyph, in device pixels.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GlyphSize {
    /// Slot width.
    pub width: u16,
    /// Slot height.
    pub height: u16,
}

impl GlyphSize {
    /// A slot of `width` x `height` device pixels.
    #[must_use]
    pub const fn new(width: u16, height: u16) -> Self {
        Self { width, height }
    }
}

/// Where one glyph's bitmap lives (kernel/03 section 3.4 `GlyphSlot`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GlyphSlot {
    /// Page index inside the atlas.
    pub page: u8,
    /// Slot origin on the page, in device pixels.
    pub x: u16,
    /// Slot origin on the page, in device pixels.
    pub y: u16,
    /// Reserved width.
    pub width: u16,
    /// Reserved height.
    pub height: u16,
    /// Rebuild generation that produced this slot. A slot from an older generation is a miss
    /// (kernel/03 section 3.4: "old generation slots count as misses, never as wrong glyphs").
    pub gen: u16,
}

/// Atlas budget (kernel/03 section 3.4: text pages `R8Unorm` 1024x1024, at most 4 text pages,
/// hard ceiling 24 MiB; the RGBA colour page arrives with the emoji slice).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AtlasConfig {
    /// Page edge length in device pixels.
    pub page_size: u16,
    /// Maximum number of text pages.
    pub max_pages: u8,
    /// Hard byte ceiling for the atlas.
    pub max_bytes: usize,
}

impl AtlasConfig {
    /// Spec page edge: 1024x1024 `R8Unorm` = 1 MiB per page.
    pub const DEFAULT_PAGE_SIZE: u16 = 1024;
    /// Spec cap: at most 4 text pages.
    pub const DEFAULT_MAX_PAGES: u8 = 4;
    /// Spec hard ceiling: 24 MiB (kernel/03 section 3.4).
    pub const DEFAULT_MAX_BYTES: usize = 24 * 1024 * 1024;
}

impl Default for AtlasConfig {
    fn default() -> Self {
        Self {
            page_size: Self::DEFAULT_PAGE_SIZE,
            max_pages: Self::DEFAULT_MAX_PAGES,
            max_bytes: Self::DEFAULT_MAX_BYTES,
        }
    }
}

/// Why the atlas could not hand out a slot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AtlasError {
    /// The requested slot does not fit an empty page, so no amount of eviction would help.
    GlyphTooLarge {
        /// Requested width.
        width: u16,
        /// Requested height.
        height: u16,
        /// Page edge length.
        page_size: u16,
    },
    /// The page budget forbids a new page (kernel/03 section 3.4: the atlas never grows to
    /// protect the RSS gate) and there is no page to reuse.
    BudgetExhausted {
        /// Page budget in bytes.
        max_bytes: usize,
        /// Page budget in pages.
        max_pages: u8,
    },
}

/// One atlas page: a shelf-packed bitmap plus the keys it holds, in insertion order.
#[derive(Clone, Debug, Default)]
struct Page {
    x: u16,
    y: u16,
    shelf_height: u16,
    keys: Vec<AtlasKey>,
}

impl Page {
    fn reset(&mut self) {
        self.x = 0;
        self.y = 0;
        self.shelf_height = 0;
        self.keys.clear();
    }

    /// Plan a placement without touching the page. `None` when the page has no room left.
    fn plan(&self, size: GlyphSize, page_size: u16) -> Option<(u16, u16, u16)> {
        let mut x = self.x;
        let mut y = self.y;
        let mut shelf = self.shelf_height;
        if u32::from(x) + u32::from(size.width) > u32::from(page_size) {
            // Close the shelf and start a new one.
            x = 0;
            y = y.saturating_add(shelf);
            shelf = 0;
        }
        if u32::from(y) + u32::from(size.height) > u32::from(page_size) {
            return None;
        }
        Some((x, y, shelf.max(size.height)))
    }

    fn commit(&mut self, x: u16, y: u16, size: GlyphSize, shelf_height: u16) {
        self.x = x.saturating_add(size.width);
        self.y = y;
        self.shelf_height = shelf_height;
    }
}

/// The glyph atlas (kernel/03 section 3.4).
#[derive(Clone, Debug)]
pub struct GlyphAtlas {
    config: AtlasConfig,
    pages: Vec<Page>,
    index: HashMap<AtlasKey, GlyphSlot>,
    /// Page indices, least recently used first. A vector, so the order can never depend on a
    /// hash iteration.
    lru: Vec<u8>,
    gen: u16,
    misses: u64,
    hits: u64,
    evictions: u64,
    evicted_keys: u64,
    rebuilds: u64,
}

impl GlyphAtlas {
    /// A new, empty atlas.
    #[must_use]
    pub fn new(config: AtlasConfig) -> Self {
        Self {
            config,
            pages: Vec::new(),
            index: HashMap::new(),
            lru: Vec::new(),
            gen: 0,
            misses: 0,
            hits: 0,
            evictions: 0,
            evicted_keys: 0,
            rebuilds: 0,
        }
    }

    /// The configuration this atlas was built with.
    #[must_use]
    pub fn config(&self) -> AtlasConfig {
        self.config
    }

    /// Number of live pages.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Bytes the live pages reserve: one byte per pixel, which is the `R8Unorm` text-page
    /// layout AR-14's grayscale AA allows (four times cheaper than an RGBA page).
    #[must_use]
    pub fn used_bytes(&self) -> usize {
        self.pages.len() * self.page_bytes()
    }

    /// Current rebuild generation. Every slot carries it, and a slot from an older generation
    /// is stale by construction.
    #[must_use]
    pub fn generation(&self) -> u16 {
        self.gen
    }

    /// Misses: lookups that had to allocate a slot (S7 failure-localisation column).
    #[must_use]
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Hits: lookups that found an existing slot.
    #[must_use]
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Eviction count: pages reclaimed by the page-level LRU.
    #[must_use]
    pub fn evictions(&self) -> u64 {
        self.evictions
    }

    /// Glyph slots dropped by eviction, summed over the reclaimed pages.
    #[must_use]
    pub fn evicted_keys(&self) -> u64 {
        self.evicted_keys
    }

    /// How many times the atlas was rebuilt (font change, size change, device loss, command).
    #[must_use]
    pub fn rebuilds(&self) -> u64 {
        self.rebuilds
    }

    /// The keys a page holds, in insertion order. Deterministic, unlike the index itself.
    #[must_use]
    pub fn keys_in_page(&self, page: u8) -> &[AtlasKey] {
        self.pages
            .get(usize::from(page))
            .map_or(&[], |p| p.keys.as_slice())
    }

    /// Look up a slot and mark its page as recently used. Does not count as a hit or a miss:
    /// only [`GlyphAtlas::get_or_insert`] moves the S7 counters.
    pub fn get(&mut self, key: &AtlasKey) -> Option<GlyphSlot> {
        let slot = self.index.get(key).copied()?;
        self.touch(slot.page);
        Some(slot)
    }

    /// Look up a slot, or reserve one for `size` and count the miss.
    ///
    /// Deterministic: the same key and size return the same slot for as long as the page is
    /// not evicted or rebuilt, and the same insertion sequence always produces the same slots.
    ///
    /// # Errors
    /// [`AtlasError::GlyphTooLarge`] when the slot cannot fit an empty page, or
    /// [`AtlasError::BudgetExhausted`] when the page budget forbids any page at all.
    pub fn get_or_insert(
        &mut self,
        key: AtlasKey,
        size: GlyphSize,
    ) -> Result<GlyphSlot, AtlasError> {
        if let Some(slot) = self.index.get(&key).copied() {
            self.hits += 1;
            self.touch(slot.page);
            return Ok(slot);
        }
        if size.width > self.config.page_size || size.height > self.config.page_size {
            return Err(AtlasError::GlyphTooLarge {
                width: size.width,
                height: size.height,
                page_size: self.config.page_size,
            });
        }
        self.misses += 1;
        let slot = self.allocate(size)?;
        self.touch(slot.page);
        self.index.insert(key, slot);
        if let Some(page) = self.pages.get_mut(usize::from(slot.page)) {
            page.keys.push(key);
        }
        Ok(slot)
    }

    /// Invalidate every slot: `gen += 1`, all pages dropped (kernel/03 section 3.4 - font file,
    /// fallback chain, size or AA preset change, device loss, explicit command). Slots handed
    /// out earlier keep their old generation, so a caller can always tell they are stale.
    pub fn rebuild(&mut self) {
        self.gen = self.gen.wrapping_add(1);
        self.rebuilds += 1;
        for page in &mut self.pages {
            page.reset();
        }
        self.pages.clear();
        self.index.clear();
        self.lru.clear();
    }

    fn page_bytes(&self) -> usize {
        usize::from(self.config.page_size) * usize::from(self.config.page_size)
    }

    fn can_add_page(&self) -> bool {
        let next = self.pages.len() + 1;
        next <= usize::from(self.config.max_pages)
            && next * self.page_bytes() <= self.config.max_bytes
    }

    fn touch(&mut self, page: u8) {
        self.lru.retain(|p| *p != page);
        self.lru.push(page);
    }

    /// Reserve a slot, reusing or evicting pages as the budget requires.
    fn allocate(&mut self, size: GlyphSize) -> Result<GlyphSlot, AtlasError> {
        loop {
            for index in 0..self.pages.len() {
                if let Some((x, y, shelf)) = self.pages[index].plan(size, self.config.page_size) {
                    let page = u8::try_from(index).unwrap_or(u8::MAX);
                    let slot = GlyphSlot {
                        page,
                        x,
                        y,
                        width: size.width,
                        height: size.height,
                        gen: self.gen,
                    };
                    self.pages[index].commit(x, y, size, shelf);
                    return Ok(slot);
                }
            }
            if self.can_add_page() {
                self.pages.push(Page::default());
                continue;
            }
            if self.pages.is_empty() {
                return Err(AtlasError::BudgetExhausted {
                    max_bytes: self.config.max_bytes,
                    max_pages: self.config.max_pages,
                });
            }
            self.evict_lru_page();
        }
    }

    /// Page-level LRU (kernel/03 section 3.4): eviction is per page, never per glyph, so the
    /// index cannot fragment. The frame pin that protects the current and previous frame's
    /// pages needs the double-frame ring from the S8/S9 slice, so this slice evicts strictly by
    /// page recency.
    fn evict_lru_page(&mut self) {
        let Some(victim) = self.lru.first().copied() else {
            // No recency record (should be unreachable while a page is live): fall back to page
            // order so eviction is still deterministic rather than panicking.
            let victim = u8::try_from(self.pages.len() - 1).unwrap_or(0);
            self.evict_page(victim);
            return;
        };
        self.evict_page(victim);
    }

    fn evict_page(&mut self, victim: u8) {
        let index = usize::from(victim);
        let Some(page) = self.pages.get_mut(index) else {
            return;
        };
        let dropped = page.keys.len();
        for key in page.keys.drain(..) {
            self.index.remove(&key);
        }
        page.reset();
        self.index.retain(|_, slot| slot.page != victim);
        self.evictions += 1;
        self.evicted_keys += u64::try_from(dropped).unwrap_or(u64::MAX);
        // The reclaimed page is now the most recently used one.
        self.gen = self.gen.wrapping_add(1);
        self.lru.retain(|p| *p != victim);
        self.lru.push(victim);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(glyph_id: u32, px_size: u16) -> AtlasKey {
        AtlasKey {
            font_id: FontId::dummy(),
            glyph_id,
            px_size,
            aa_mode: AaMode::Sharp,
        }
    }

    fn small() -> AtlasConfig {
        AtlasConfig {
            page_size: 16,
            max_pages: 2,
            max_bytes: 4096,
        }
    }

    #[test]
    fn the_same_key_returns_the_same_slot_and_a_new_px_size_a_new_slot() {
        let mut atlas = GlyphAtlas::new(AtlasConfig::default());
        let a = atlas
            .get_or_insert(key(36, 16), GlyphSize::new(8, 16))
            .unwrap();
        let again = atlas
            .get_or_insert(key(36, 16), GlyphSize::new(8, 16))
            .unwrap();
        assert_eq!(a, again);
        let b = atlas
            .get_or_insert(key(36, 18), GlyphSize::new(8, 18))
            .unwrap();
        assert_ne!(a, b);
        assert_eq!(atlas.misses(), 2);
        assert_eq!(atlas.hits(), 1);
        assert_eq!(atlas.generation(), 0);
    }

    #[test]
    fn the_aa_preset_is_part_of_the_key() {
        let mut atlas = GlyphAtlas::new(small());
        let sharp = atlas
            .get_or_insert(key(36, 16), GlyphSize::new(8, 8))
            .unwrap();
        let soft_key = AtlasKey {
            aa_mode: AaMode::Soft,
            ..key(36, 16)
        };
        let soft = atlas.get_or_insert(soft_key, GlyphSize::new(8, 8)).unwrap();
        assert_ne!(sharp, soft);
    }

    #[test]
    fn allocation_is_deterministic_for_the_same_sequence() {
        let sizes = [
            GlyphSize::new(8, 8),
            GlyphSize::new(4, 8),
            GlyphSize::new(8, 16),
        ];
        let mut first = GlyphAtlas::new(small());
        let mut second = GlyphAtlas::new(small());
        let mut seen_first = Vec::new();
        let mut seen_second = Vec::new();
        for glyph_id in 0..40 {
            let size = sizes[usize::try_from(glyph_id).unwrap() % sizes.len()];
            seen_first.push(first.get_or_insert(key(glyph_id, 16), size).unwrap());
            seen_second.push(second.get_or_insert(key(glyph_id, 16), size).unwrap());
        }
        assert_eq!(seen_first, seen_second);
        assert_eq!(first.generation(), second.generation());
        assert_eq!(first.page_count(), second.page_count());
    }

    #[test]
    fn a_full_atlas_evicts_the_least_recently_used_page() {
        // 16x16 pages with 8x8 slots hold four slots each; two pages hold eight.
        let mut atlas = GlyphAtlas::new(small());
        for glyph_id in 0..8 {
            atlas
                .get_or_insert(key(glyph_id, 16), GlyphSize::new(8, 8))
                .unwrap();
        }
        assert_eq!(atlas.page_count(), 2);
        assert_eq!(atlas.evictions(), 0);
        // Touch page 1 by looking up a key that lives there, so page 0 is the LRU victim.
        assert!(atlas.get(&key(7, 16)).is_some());
        let new_slot = atlas
            .get_or_insert(key(8, 16), GlyphSize::new(8, 8))
            .unwrap();
        assert_eq!(atlas.evictions(), 1);
        assert_eq!(atlas.evicted_keys(), 4);
        assert_eq!(atlas.generation(), 1);
        assert_eq!(new_slot.page, 0, "the reclaimed page is reused");
        assert_eq!(new_slot.gen, 1);
        // The evicted keys are gone rather than silently pointing at stale pixels.
        assert!(atlas.get(&key(0, 16)).is_none());
        assert!(atlas.get(&key(7, 16)).is_some());
    }

    #[test]
    fn rebuild_bumps_the_generation_and_invalidates_every_slot() {
        let mut atlas = GlyphAtlas::new(small());
        let before = atlas
            .get_or_insert(key(36, 16), GlyphSize::new(8, 8))
            .unwrap();
        atlas.rebuild();
        assert_eq!(atlas.generation(), 1);
        assert_eq!(atlas.rebuilds(), 1);
        assert_eq!(atlas.page_count(), 0);
        assert!(atlas.get(&key(36, 16)).is_none());
        let after = atlas
            .get_or_insert(key(36, 16), GlyphSize::new(8, 8))
            .unwrap();
        assert_eq!(after.gen, 1);
        assert_ne!(before.gen, after.gen);
    }

    #[test]
    fn a_slot_larger_than_a_page_is_refused() {
        let mut atlas = GlyphAtlas::new(small());
        assert_eq!(
            atlas.get_or_insert(key(36, 16), GlyphSize::new(17, 8)),
            Err(AtlasError::GlyphTooLarge {
                width: 17,
                height: 8,
                page_size: 16,
            })
        );
        assert_eq!(atlas.misses(), 0, "a refused slot is not a miss");
        assert_eq!(atlas.page_count(), 0);
    }

    #[test]
    fn a_page_budget_of_zero_is_reported_not_panicked() {
        let mut atlas = GlyphAtlas::new(AtlasConfig {
            page_size: 16,
            max_pages: 0,
            max_bytes: 0,
        });
        assert_eq!(
            atlas.get_or_insert(key(36, 16), GlyphSize::new(8, 8)),
            Err(AtlasError::BudgetExhausted {
                max_bytes: 0,
                max_pages: 0,
            })
        );
        assert_eq!(atlas.page_count(), 0);
        assert_eq!(atlas.used_bytes(), 0);
    }

    #[test]
    fn the_byte_ceiling_limits_the_page_count() {
        // One 16x16 page is 256 bytes, so a 512-byte ceiling allows exactly two pages even
        // though max_pages would allow four.
        let mut atlas = GlyphAtlas::new(AtlasConfig {
            page_size: 16,
            max_pages: 4,
            max_bytes: 512,
        });
        for glyph_id in 0..12 {
            atlas
                .get_or_insert(key(glyph_id, 16), GlyphSize::new(8, 8))
                .unwrap();
        }
        assert_eq!(atlas.page_count(), 2);
        assert_eq!(atlas.used_bytes(), 512);
        assert!(atlas.evictions() >= 1);
    }
}
