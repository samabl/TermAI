//! S7 glyph atlas: `AtlasKey` -> deterministic slot allocation *and* rasterisation (kernel/03
//! section 3.4).
//!
//! The atlas is the second stage after shaping: S6 says *which* glyphs a row draws and into
//! which cells, S7 says *where* their bitmaps live and what those bitmaps are. This slice owns
//! the key, the page/slot bookkeeping, the eviction rule, the three failure-localisation
//! counters the spec's S7 row asks for (`miss` count, eviction count, rebuild generation) and
//! the rasterisation itself: an [`AtlasKey`] plus a resolved face produces an 8-bit alpha
//! coverage bitmap that is blitted into the page at the slot the allocator handed out, and read
//! back with its ink box and slot rect.
//!
//! Engines (kernel/03 K-05, admitted by ADR-0027 D1): `swash` is the only rasterisation /
//! colour-glyph engine. AR-14 fixes the rasterisation settings - grayscale anti-aliasing
//! (`zeno::Format::Alpha`, one coverage byte per pixel, never `Format::Subpixel`) and **hinting
//! OFF** (`.hint(false)`). Colour glyphs (COLR/CBDT, DC-17) are **refused** with a structured
//! error instead of being approximated in gray: kernel/03 section 3.4 puts them on an RGBA8
//! colour page, which this slice does not own. See [`AtlasError::ColorGlyph`].
//!
//! The atlas still owns no GPU resource: a page is a CPU buffer in exactly the `R8Unorm`
//! layout the S8 upload will read (`page_size * page_size` bytes, one per pixel), so a slot rect
//! read back from the atlas is where the pixels really are. That is also why this module is
//! testable with no GPU and no window.
//!
//! Determinism: allocation is a pure function of the insertion sequence and the glyph sizes, and
//! a bitmap is a pure function of (face, glyph id, device pixel size, AA preset). Pages are
//! packed by a shelf allocator in page order, the index is a `HashMap` that is never iterated
//! for allocation (its iteration order must never leak into a slot), and the LRU is a vector
//! rather than a hash set. The same sequence therefore produces byte-identical slots and
//! byte-identical pixels.

use std::collections::HashMap;
use std::fmt;

use swash::scale::image::{Content, Image};
use swash::scale::{Render, ScaleContext, Scaler, Source, StrikeWith};
use swash::zeno::{Format, Style, Vector};

use crate::shape::{AaMode, FontFace, FontId};

/// The atlas primary key (kernel/03 section 3.4). Every field participates in the hash: a
/// different glyph id, font or device pixel size is a different bitmap.
///
/// The AA preset separates two cache entries but **not yet two coverage curves**: kernel/03
/// section 3.4 / AR-14 fix the two presets as "Sharp | Soft, both grayscale" and define no
/// coverage mechanism for the difference, so this slice rasterises both through the identical
/// grayscale path (hinting off) and keeps the key field so the AA-preset slice can change one
/// curve without invalidating the other's entries. Measured on the ATLAS test font, `Sharp` and
/// `Soft` bitmaps for the same glyph are byte-identical today - that is a documented deferral,
/// not a claim about how the two presets will differ.
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

/// One rasterised glyph as it lives in the atlas: the coverage bitmap, where it sits on the page
/// and where its ink box sits relative to the pen origin.
///
/// The bytes are **8-bit single-channel alpha coverage** (AR-14's grayscale AA): exactly
/// `width * height` of them, row-major, one per device pixel - never RGB, never a subpixel mask
/// with three samples per pixel. `data` is read back out of the page, so it is the same bytes the
/// S8 upload will send.
///
/// `left` / `top` are the swash placement: the offset from the glyph's pen origin (on the
/// baseline) to the left edge and to the top edge of the ink box, in device pixels, `top`
/// positive upwards. Together with the slot rect these are what RP-05's "glyph bitmap origin vs
/// cell box origin" measurement needs - this slice exposes them but does not yet do the cell-box
/// placement, so no <=0.5px number can be claimed from it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GlyphBitmap {
    /// The key this bitmap belongs to.
    pub key: AtlasKey,
    /// The slot the bitmap occupies on its page.
    pub slot: GlyphSlot,
    /// Bitmap width in device pixels (equals `slot.width`).
    pub width: u16,
    /// Bitmap height in device pixels (equals `slot.height`).
    pub height: u16,
    /// Pen origin to ink-box left edge, in device pixels (swash placement `left`).
    pub left: i16,
    /// Pen origin to ink-box top edge, in device pixels, positive up (swash placement `top`).
    pub top: i16,
    /// `width * height` coverage bytes, row-major, one byte per pixel.
    pub data: Vec<u8>,
}

impl GlyphBitmap {
    /// A bitmap with no ink area at all. A blank glyph (a space) legitimately rasterises to this;
    /// a glyph that produced no image at all is reported as [`AtlasError::NoOutline`] instead, so
    /// an empty slot is never stored silently.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0 || self.data.is_empty()
    }

    /// Whether any pixel carries coverage. A glyph with an ink box but no set pixel is possible
    /// at very small pixel sizes; it is reported here rather than hidden.
    #[must_use]
    pub fn has_ink(&self) -> bool {
        self.data.iter().any(|byte| *byte != 0)
    }

    /// Number of pixels carrying full coverage (`== 255`), for the "is this really an
    /// anti-aliased mask" checks.
    #[must_use]
    pub fn opaque_pixels(&self) -> usize {
        self.data.iter().filter(|byte| **byte == 255).count()
    }

    /// A stable digest of the bitmap geometry and bytes, for determinism evidence. FNV-1a over
    /// the canonical fields: no address, no iteration order, no timestamp.
    #[must_use]
    pub fn digest(&self) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |bytes: &[u8]| {
            for byte in bytes {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        mix(&self.width.to_le_bytes());
        mix(&self.height.to_le_bytes());
        mix(&self.left.to_le_bytes());
        mix(&self.top.to_le_bytes());
        mix(&self.data);
        hash
    }
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

/// Why the atlas could not hand out a slot or could not rasterise one.
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
    /// The resolved face does not match the font the key names. Rasterising one face's glyph id
    /// against another face's outlines is exactly the "right slot, wrong glyph" failure the key
    /// exists to prevent, so it is refused rather than drawn.
    FontMismatch {
        /// The font the key names.
        key_font: FontId,
        /// The face the rasteriser was handed.
        face: FontId,
    },
    /// `swash` cannot parse the face bytes at the given collection index (K-05's rasteriser needs
    /// its own view of the same bytes `rustybuzz` shaped from).
    UnreadableFont {
        /// The face that could not be parsed.
        font_id: FontId,
    },
    /// The key's glyph id does not fit `swash`'s 16-bit glyph id space.
    GlyphOutOfRange {
        /// The out-of-range glyph id.
        glyph_id: u32,
    },
    /// The key asks for a rasterisation at zero device pixels per em. `swash` reads `size(0)` as
    /// "unscaled font units", which would silently produce a glyph hundreds of pixels wide.
    ZeroPixelSize,
    /// The glyph is a colour or bitmap glyph (COLR/CBDT - kernel/03 section 3.4, DC-17).
    ///
    /// **Refused, not approximated.** Colour glyphs belong on the RGBA8 colour page (4 MiB) the
    /// emoji slice owns; this slice owns only the `R8Unorm` text page and AR-14 promises no
    /// subpixel AA. Squeezing a COLR/CBDT glyph into a one-channel mask would be a wrong picture
    /// behind a right-looking slot, so the atlas reports it and the caller decides - which is also
    /// why S6's spans already carry `glyph_flag::COLOR`.
    ColorGlyph {
        /// The face that owns the colour glyph.
        font_id: FontId,
        /// The colour glyph id.
        glyph_id: u32,
    },
    /// Neither an outline nor a bitmap source produced an image for this glyph: there is nothing
    /// to store, and storing an empty slot would be a silent hole in the page.
    NoOutline {
        /// The face that had no image for the glyph.
        font_id: FontId,
        /// The glyph id.
        glyph_id: u32,
    },
    /// The rasteriser returned something other than one 8-bit coverage byte per pixel (`channels`
    /// is the measured bytes per pixel). AR-14's grayscale AA is one channel; a subpixel mask
    /// would be three and an RGBA bitmap four.
    NotSingleChannel {
        /// The glyph id.
        glyph_id: u32,
        /// Measured bytes per pixel.
        channels: u8,
    },
    /// The bitmap is larger than a whole page, so no page could ever hold it.
    BitmapTooLarge {
        /// Rasterised width in device pixels.
        width: u32,
        /// Rasterised height in device pixels.
        height: u32,
        /// Page edge length.
        page_size: u16,
    },
}

/// One atlas page: a shelf-packed `R8Unorm` pixel buffer plus the keys it holds, in insertion
/// order.
///
/// `pixels` is the real page: `page_size * page_size` bytes, one coverage byte per device pixel,
/// allocated up front so the page's byte cost is the page's byte cost whether or not a glyph has
/// been rasterised into it yet - the same number a GPU `R8Unorm` texture of that size costs.
#[derive(Clone, Debug, Default)]
struct Page {
    x: u16,
    y: u16,
    shelf_height: u16,
    keys: Vec<AtlasKey>,
    /// Rasterised-glyph bearings on this page: `(left, top)` in device pixels relative to the pen
    /// origin. A key reserved by [`GlyphAtlas::get_or_insert`] has no entry - it owns a slot but
    /// no bitmap - so `bitmap()` can never hand back invented placement for a slot with no pixels.
    bearings: HashMap<AtlasKey, (i16, i16)>,
    /// Coverage bytes of the bitmaps stored on this page (the sum of their `width * height`).
    bitmap_bytes: usize,
    pixels: Vec<u8>,
}

impl Page {
    /// A page with its pixel buffer allocated and zeroed.
    fn new(page_size: u16) -> Self {
        let bytes = usize::from(page_size) * usize::from(page_size);
        Self {
            pixels: vec![0; bytes],
            ..Self::default()
        }
    }

    fn reset(&mut self) {
        self.x = 0;
        self.y = 0;
        self.shelf_height = 0;
        self.keys.clear();
        self.bearings.clear();
        self.bitmap_bytes = 0;
        self.pixels.fill(0);
    }

    /// Copy one glyph bitmap into its slot. The page *is* the texture the GPU will upload
    /// (kernel/03 section 3.4), so the bitmap lives exactly where the slot says it does.
    fn blit(&mut self, slot: GlyphSlot, data: &[u8], page_size: u16) {
        let width = usize::from(slot.width);
        let height = usize::from(slot.height);
        let stride = usize::from(page_size);
        if width == 0 || height == 0 {
            return;
        }
        for row in 0..height {
            let dst = (usize::from(slot.y) + row) * stride + usize::from(slot.x);
            let src = row * width;
            if let (Some(dst), Some(src)) = (
                self.pixels.get_mut(dst..dst + width),
                data.get(src..src + width),
            ) {
                dst.copy_from_slice(src);
            }
        }
    }

    /// Read one slot's pixels back out of the page, row-major.
    fn read_back(&self, slot: GlyphSlot, page_size: u16) -> Option<Vec<u8>> {
        let width = usize::from(slot.width);
        let height = usize::from(slot.height);
        let stride = usize::from(page_size);
        let mut out = Vec::with_capacity(width * height);
        for row in 0..height {
            let start = (usize::from(slot.y) + row) * stride + usize::from(slot.x);
            out.extend_from_slice(self.pixels.get(start..start + width)?);
        }
        Some(out)
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
///
/// The `swash` scaler/render context is part of the atlas because the rasteriser is a per-atlas
/// engine, not a per-glyph one: it caches parsed font proxies, and rebuilding it per glyph would
/// re-parse the font on the hot path. It holds no pixels - every pixel lives in `pages`.
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
    /// Bytes of the live pages covered by stored glyph bitmaps. The gap to [`Self::used_bytes`]
    /// is shelf padding and page slack, and it is the number that would collapse if the
    /// rasteriser ever stored the same bitmap under two keys.
    bitmap_bytes: usize,
    /// swash's scaler/render context (kernel/03 K-05: swash is the only rasterisation engine).
    raster: ScaleContext,
}

impl fmt::Debug for GlyphAtlas {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GlyphAtlas")
            .field("config", &self.config)
            .field("pages", &self.pages.len())
            .field("entries", &self.index.len())
            .field("gen", &self.gen)
            .field("misses", &self.misses)
            .field("hits", &self.hits)
            .field("evictions", &self.evictions)
            .field("evicted_keys", &self.evicted_keys)
            .field("rebuilds", &self.rebuilds)
            .field("used_bytes", &self.used_bytes())
            .field("bitmap_bytes", &self.bitmap_bytes)
            .finish()
    }
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
            bitmap_bytes: 0,
            raster: ScaleContext::new(),
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
    ///
    /// This is the **page-level** number kernel/03 section 3.4 budgets (a 1024x1024 page is
    /// 1 MiB whether one glyph or a thousand are in it), i.e. the cost of the texture the S8
    /// upload will allocate. The bytes actually covered by glyph bitmaps are
    /// [`Self::bitmap_bytes`]; the two are related by `bitmap_bytes() <= used_bytes()`.
    #[must_use]
    pub fn used_bytes(&self) -> usize {
        self.pages.iter().map(|page| page.pixels.len()).sum()
    }

    /// Bytes of the live pages actually covered by stored glyph bitmaps: the sum of every
    /// rasterised glyph's `width * height`.
    ///
    /// Eviction and [`Self::rebuild`] keep this in step with the pages automatically, so it can
    /// never account for a bitmap that is no longer reachable.
    #[must_use]
    pub fn bitmap_bytes(&self) -> usize {
        self.bitmap_bytes
    }

    /// The bitmap bytes a single page currently accounts for.
    #[must_use]
    pub fn page_bitmap_bytes(&self, page: u8) -> usize {
        self.pages
            .get(usize::from(page))
            .map_or(0, |p| p.bitmap_bytes)
    }

    /// The raw page pixels: `page_size * page_size` bytes, one per device pixel, exactly the
    /// buffer the S8 upload reads. Empty for a page that does not exist.
    #[must_use]
    pub fn page_pixels(&self, page: u8) -> &[u8] {
        self.pages
            .get(usize::from(page))
            .map_or(&[], |p| p.pixels.as_slice())
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

    /// Rasterise one glyph into the atlas, or return the bitmap already stored for that key
    /// (kernel/03 section 3.4 `Rasterizer::rasterize`).
    ///
    /// The whole point of S7: an [`AtlasKey`] plus the resolved face it names become real
    /// coverage bytes, blitted into the page at the slot the allocator handed out. The slot's
    /// size is the glyph's **ink box**, not a caller's guess, so the reported rect and the
    /// bitmap can never disagree.
    ///
    /// AR-14 fixes the rasterisation: grayscale AA (`Format::Alpha`) and hinting **off**. K-05
    /// fixes the engine: `swash`. Colour glyphs are refused ([`AtlasError::ColorGlyph`]).
    ///
    /// Deterministic: the same key and the same face bytes give byte-identical coverage, because
    /// nothing in the path depends on time, address or a hash iteration order.
    ///
    /// # Errors
    /// [`AtlasError::FontMismatch`] / [`AtlasError::UnreadableFont`] when the face is not the one
    /// the key names, [`AtlasError::ZeroPixelSize`] / [`AtlasError::GlyphOutOfRange`] for a key
    /// the rasteriser cannot honour, [`AtlasError::ColorGlyph`] for a COLR/CBDT glyph,
    /// [`AtlasError::NoOutline`] when no source produced an image,
    /// [`AtlasError::NotSingleChannel`] when the result is not one coverage byte per pixel,
    /// [`AtlasError::BitmapTooLarge`] when the ink box cannot fit a page, and the two allocation
    /// errors of [`Self::get_or_insert`].
    pub fn rasterize(
        &mut self,
        key: AtlasKey,
        font: &FontFace<'_>,
    ) -> Result<GlyphBitmap, AtlasError> {
        if let Some(stored) = self.bitmap(&key) {
            self.hits += 1;
            self.touch(stored.slot.page);
            return Ok(stored);
        }
        let image = self.render_glyph(key, font)?;
        let width = image.placement.width;
        let height = image.placement.height;
        if width > u32::from(self.config.page_size) || height > u32::from(self.config.page_size) {
            return Err(AtlasError::BitmapTooLarge {
                width,
                height,
                page_size: self.config.page_size,
            });
        }
        // AR-14's grayscale AA is one channel. Anything else (a subpixel mask, an RGBA bitmap)
        // would still *look* like a bitmap, so it is refused at the boundary rather than stored.
        if image.content != Content::Mask {
            let pixels = u64::from(width) * u64::from(height);
            let channels = if pixels == 0 {
                0
            } else {
                u8::try_from(u64::try_from(image.data.len()).unwrap_or(u64::MAX) / pixels)
                    .unwrap_or(u8::MAX)
            };
            return Err(AtlasError::NotSingleChannel {
                glyph_id: key.glyph_id,
                channels,
            });
        }
        let width = u16::try_from(width).unwrap_or(u16::MAX);
        let height = u16::try_from(height).unwrap_or(u16::MAX);
        let data = image.data;
        // A key that was reserved by `get_or_insert` but never rasterised holds a slot sized from
        // the caller's guess, not from the ink box; that reservation is replaced here. The shelf
        // area it occupied is wasted until the page is rebuilt - the two paths are not meant to
        // be mixed for one key, and the rasteriser is the production path.
        self.drop_reservation(&key);
        let slot = self.allocate(GlyphSize::new(width, height))?;
        self.misses += 1;
        let page_size = self.config.page_size;
        let left = clamp_i16(image.placement.left);
        let top = clamp_i16(image.placement.top);
        if let Some(page) = self.pages.get_mut(usize::from(slot.page)) {
            page.blit(slot, &data, page_size);
            page.keys.push(key);
            page.bearings.insert(key, (left, top));
            page.bitmap_bytes += data.len();
        }
        self.bitmap_bytes += data.len();
        self.index.insert(key, slot);
        self.touch(slot.page);
        Ok(GlyphBitmap {
            key,
            slot,
            width,
            height,
            left,
            top,
            data,
        })
    }

    /// Read one glyph's bitmap back out of the page it was blitted into: coverage bytes, ink box
    /// and slot rect. `None` when the key has no bitmap - it was never rasterised, or its page
    /// was evicted - so a caller can never mistake a stale slot for pixels.
    #[must_use]
    pub fn bitmap(&self, key: &AtlasKey) -> Option<GlyphBitmap> {
        let slot = *self.index.get(key)?;
        let page = self.pages.get(usize::from(slot.page))?;
        let (left, top) = *page.bearings.get(key)?;
        Some(GlyphBitmap {
            key: *key,
            slot,
            width: slot.width,
            height: slot.height,
            left,
            top,
            data: page.read_back(slot, self.config.page_size)?,
        })
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
        self.bitmap_bytes = 0;
    }

    /// Drive swash's scaler/render path for one key. Split out so the borrow of the atlas's
    /// scaler context ends before the allocation touches the pages.
    fn render_glyph(&mut self, key: AtlasKey, font: &FontFace<'_>) -> Result<Image, AtlasError> {
        if key.font_id != font.id {
            return Err(AtlasError::FontMismatch {
                key_font: key.font_id,
                face: font.id,
            });
        }
        if key.px_size == 0 {
            return Err(AtlasError::ZeroPixelSize);
        }
        let glyph_id = u16::try_from(key.glyph_id).map_err(|_| AtlasError::GlyphOutOfRange {
            glyph_id: key.glyph_id,
        })?;
        let font_ref = swash::FontRef::from_index(font.data, font.index as usize).ok_or(
            AtlasError::UnreadableFont {
                font_id: key.font_id,
            },
        )?;
        // The AR-14 scaler configuration, and the only place it is decided: device pixels per em
        // from the key, and hinting OFF (a grid-fitted outline is the "系统栅格化的 hinting" the
        // ADR-0014 decision 3 / AR-14 pair gives up, and the ≤0.5px alignment rule is measured on
        // the unhinted metric, not on a snapped one).
        let mut scaler = self
            .raster
            .builder(font_ref)
            .size(f32::from(key.px_size))
            .hint(false)
            .build();
        if is_color_glyph(&mut scaler, glyph_id) {
            return Err(AtlasError::ColorGlyph {
                font_id: key.font_id,
                glyph_id: key.glyph_id,
            });
        }
        let mut render = Render::new(&[Source::Outline]);
        render
            // AR-14: grayscale anti-aliasing = one coverage byte per pixel. `Format::Subpixel`
            // (three samples per pixel) is the ClearType-shaped path AR-14 explicitly does not
            // promise, and `Content::Mask` is asserted on the way out of `rasterize`.
            .format(Format::Alpha)
            .style(Style::default())
            .offset(Vector::new(0.0, 0.0));
        render
            .render(&mut scaler, glyph_id)
            .ok_or(AtlasError::NoOutline {
                font_id: key.font_id,
                glyph_id: key.glyph_id,
            })
    }

    /// Drop a slot that was reserved but never rasterised, so the rasteriser can size the slot
    /// from the ink box instead. The key leaves the page's insertion-order list with it, so the
    /// eviction bookkeeping stays honest.
    fn drop_reservation(&mut self, key: &AtlasKey) {
        if let Some(slot) = self.index.remove(key) {
            if let Some(page) = self.pages.get_mut(usize::from(slot.page)) {
                page.keys.retain(|held| held != key);
            }
        }
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
                self.pages.push(Page::new(self.config.page_size));
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
        let dropped_bytes = page.bitmap_bytes;
        for key in page.keys.drain(..) {
            self.index.remove(&key);
        }
        page.reset();
        self.index.retain(|_, slot| slot.page != victim);
        // The evicted page's bitmaps are gone with it, so the bitmap accounting must not keep
        // counting bytes that no reader can reach any more.
        self.bitmap_bytes = self.bitmap_bytes.saturating_sub(dropped_bytes);
        self.evictions += 1;
        self.evicted_keys += u64::try_from(dropped).unwrap_or(u64::MAX);
        // The reclaimed page is now the most recently used one.
        self.gen = self.gen.wrapping_add(1);
        self.lru.retain(|p| *p != victim);
        self.lru.push(victim);
    }
}

/// Clamp a swash placement offset into the `i16` bearing space kernel/03 section 3.4 uses
/// (`GlyphSlot.bearing`). A real glyph never comes close; a corrupt font that reports a
/// five-digit offset is reported at the edge of the representable range instead of wrapping into
/// a plausible-looking bearing.
fn clamp_i16(value: i32) -> i16 {
    i16::try_from(value).unwrap_or(if value < 0 { i16::MIN } else { i16::MAX })
}

/// Whether this face carries the glyph as a colour/bitmap glyph (COLR/COLRv1 layers or CBDT/sbix
/// bitmaps). Kernel/03 K-05 makes `swash` the colour-glyph engine; kernel/03 section 3.4 and
/// DC-17 put the result on the RGBA8 colour page, which this slice refuses rather than
/// approximating in gray.
fn is_color_glyph(scaler: &mut Scaler<'_>, glyph_id: u16) -> bool {
    (scaler.has_color_outlines() && scaler.scale_color_outline(glyph_id).is_some())
        || (scaler.has_color_bitmaps()
            && scaler
                .scale_color_bitmap(glyph_id, StrikeWith::BestFit)
                .is_some())
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

    #[test]
    fn a_pure_allocation_owns_a_slot_but_no_bitmap_and_no_page_pixels() {
        // The allocation-only path (used by the S7 slice that preceded rasterisation and by any
        // caller that only needs a rect) must never hand back invented pixels: `bitmap()` and
        // `page_bitmap_bytes()` stay empty/zero for it, so "slot" and "bitmap" cannot be confused.
        let mut atlas = GlyphAtlas::new(small());
        let slot = atlas
            .get_or_insert(key(36, 16), GlyphSize::new(8, 8))
            .unwrap();
        assert!(atlas.bitmap(&key(36, 16)).is_none());
        assert_eq!(atlas.bitmap_bytes(), 0);
        assert_eq!(atlas.page_bitmap_bytes(slot.page), 0);
        // The page still owns its full pixel buffer: the bytes are the page's, rasterised or not.
        assert_eq!(atlas.page_pixels(slot.page).len(), 16 * 16);
        assert!(atlas.page_pixels(slot.page).iter().all(|byte| *byte == 0));
        assert_eq!(atlas.used_bytes(), 256);
    }

    #[test]
    fn the_rebuild_clears_the_bitmap_accounting_with_the_pages() {
        let mut atlas = GlyphAtlas::new(small());
        atlas
            .get_or_insert(key(36, 16), GlyphSize::new(8, 8))
            .unwrap();
        atlas.rebuild();
        assert_eq!(atlas.bitmap_bytes(), 0);
        assert_eq!(atlas.page_pixels(0).len(), 0);
        assert_eq!(atlas.used_bytes(), 0);
    }

    #[test]
    fn an_evicted_page_takes_its_bitmap_accounting_with_it() {
        // Sixteen 4x4 slots fill a 16x16 page exactly. With a one-page budget the seventeenth
        // glyph must evict, and the evicted page's bitmap bytes must leave the accounting with it.
        let mut atlas = GlyphAtlas::new(AtlasConfig {
            page_size: 16,
            max_pages: 1,
            max_bytes: 4096,
        });
        for glyph_id in 0..16 {
            atlas
                .get_or_insert(key(glyph_id, 16), GlyphSize::new(4, 4))
                .unwrap();
        }
        assert_eq!(atlas.page_count(), 1);
        atlas
            .get_or_insert(key(16, 16), GlyphSize::new(4, 4))
            .unwrap();
        assert_eq!(atlas.evictions(), 1);
        assert_eq!(atlas.evicted_keys(), 16);
        // Nothing was ever rasterised, so the bitmap accounting is zero in both pages rather
        // than negative or stale after the eviction.
        assert_eq!(atlas.bitmap_bytes(), 0);
        assert_eq!(atlas.page_bitmap_bytes(0), 0);
    }
}
