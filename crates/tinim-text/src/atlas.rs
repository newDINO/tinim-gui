//! A shelf-packed RGBA8 atlas that uploads glyphs as individual texture patches.

use tinim_paint::{ImageDelta, Rect, TextureId, TexturesDelta, pos2};

use crate::font::FontId;
use crate::hash::HashMap;
use crate::packer::ShelfPacker;

/// Space kept around every glyph and around the reserved white texel.
const PADDING: u32 = 1;

/// Size of the opaque white texel reserved at `(0, 0)`.
const WHITE_SIZE: u32 = 1;

/// Smallest atlas side that can hold the white texel and its padding.
const MIN_ATLAS_SIZE: u32 = WHITE_SIZE + PADDING;

/// The RGBA8 value of the reserved white texel.
const WHITE_PIXEL: [u8; 4] = [255, 255, 255, 255];

/// The identity of a rasterized glyph in the atlas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct GlyphKey {
    pub font: FontId,
    pub glyph_id: u32,
    pub ppem: u32,
}

/// Where a glyph lives in the atlas and how it is positioned relative to the pen.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GlyphSlot {
    /// Normalized texture coordinates covering the glyph's full texel footprint.
    pub uv: Rect,
    /// Horizontal offset from the pen position, in physical pixels.
    pub left: f32,
    /// Distance from the baseline to the top edge, in physical pixels.
    pub top: f32,
    /// Width in physical pixels.
    pub width: f32,
    /// Height in physical pixels.
    pub height: f32,
    /// True for color glyphs (bitmaps and COLRv0), which ignore the text color.
    pub color: bool,
}

/// A glyph rasterized into RGBA8.
///
/// For coverage glyphs the RGB channels are opaque white and alpha is the coverage. For color
/// glyphs the RGB channels are *straight sRGB* color and alpha is the coverage. The renderer
/// samples the atlas through an sRGB texture, so the sampler converts RGB to linear space.
pub(crate) struct RasterGlyph {
    pub width: u32,
    pub height: u32,
    pub left: f32,
    pub top: f32,
    pub color: bool,
    pub pixels: Vec<u8>,
}

pub(crate) struct Atlas {
    texture_id: TextureId,
    packer: ShelfPacker,
    slots: HashMap<GlyphKey, GlyphSlot>,
    /// Cached glyphs in insertion order, used to re-pack the atlas when it grows.
    order: Vec<GlyphKey>,
    /// Glyph pixels waiting to be uploaded. Each entry is the exact rectangle of one glyph, so
    /// the atlas never has to keep a full CPU-side copy of the texture.
    uploads: Vec<ImageDelta>,
    /// The texture size the renderer currently has, or `None` before it has been told about the
    /// atlas at all.
    ///
    /// Comparing this against the current size is enough to detect both the initial allocation
    /// and every later growth, without keeping separate "created" and "recreate" flags.
    uploaded_size: Option<[u32; 2]>,
    /// Set by [`Atlas::ensure_texture`] so a texture is allocated even when no glyph is cached.
    requested: bool,
}

impl Atlas {
    pub(crate) fn new(texture_id: TextureId, width: u32, height: u32, max_size: u32) -> Self {
        let width = width.max(MIN_ATLAS_SIZE);
        let height = height.max(MIN_ATLAS_SIZE);
        let max_size = max_size.max(width).max(height);
        let mut packer = ShelfPacker::new(width, height, max_size, PADDING);
        let reserved = packer.reserve_top_left(WHITE_SIZE, WHITE_SIZE);
        debug_assert!(
            reserved.is_some(),
            "an atlas must be large enough for the white texel"
        );
        Self {
            texture_id,
            packer,
            slots: HashMap::default(),
            order: Vec::new(),
            uploads: Vec::new(),
            uploaded_size: None,
            requested: false,
        }
    }

    pub(crate) fn texture_id(&self) -> TextureId {
        self.texture_id
    }

    /// The current atlas size, in pixels.
    pub(crate) fn size(&self) -> [u32; 2] {
        [self.packer.width(), self.packer.height()]
    }

    /// Ensures the texture is created on the next [`Atlas::take_textures`], even if no glyph has
    /// been rasterized.
    pub(crate) fn ensure_texture(&mut self) {
        self.requested = true;
    }

    pub(crate) fn get(&self, key: &GlyphKey) -> Option<GlyphSlot> {
        self.slots.get(key).copied()
    }

    /// Whether a glyph of this size could fit even at the maximum atlas size.
    ///
    /// The white texel and its padding are reserved, so the glyph must either fit beside it on
    /// the first shelf or start a new shelf below it.
    pub(crate) fn can_ever_fit(&self, width: u32, height: u32) -> bool {
        let max_size = self.packer.max_size();
        let reserved = WHITE_SIZE.saturating_add(self.packer.padding());
        let fits_beside = height <= WHITE_SIZE
            && reserved
                .saturating_add(width)
                .saturating_add(self.packer.padding())
                <= max_size;
        let fits_below = reserved
            .saturating_add(height)
            .saturating_add(self.packer.padding())
            <= max_size;
        fits_beside || fits_below
    }

    /// Reserves a slot for a glyph and queues its pixels for upload.
    ///
    /// Returns `None` when there is no room left. The caller is expected to reset the atlas and
    /// try again.
    pub(crate) fn alloc(&mut self, key: GlyphKey, glyph: RasterGlyph) -> Option<GlyphSlot> {
        if let Some(slot) = self.slots.get(&key) {
            return Some(*slot);
        }

        let RasterGlyph {
            width,
            height,
            left,
            top,
            color,
            pixels,
        } = glyph;
        if width == 0 || height == 0 {
            return None;
        }

        let (x, y) = self.packer.allocate(width, height)?;
        self.uploads
            .push(ImageDelta::partial([x, y], [width, height], pixels));

        let inverse_width = 1.0 / self.packer.width() as f32;
        let inverse_height = 1.0 / self.packer.height() as f32;
        let slot = GlyphSlot {
            uv: Rect::from_min_max(
                pos2(x as f32 * inverse_width, y as f32 * inverse_height),
                pos2(
                    (x + width) as f32 * inverse_width,
                    (y + height) as f32 * inverse_height,
                ),
            ),
            left,
            top,
            width: width as f32,
            height: height as f32,
            color,
        };
        self.slots.insert(key, slot);
        self.order.push(key);
        Some(slot)
    }

    /// The cached glyphs, in the order they were first rasterized.
    pub(crate) fn keys(&self) -> Vec<GlyphKey> {
        self.order.clone()
    }

    /// Whether the atlas can still double its dimensions.
    pub(crate) fn can_grow(&self) -> bool {
        self.packer.can_grow()
    }

    /// Doubles the atlas dimensions and forgets every slot so the caller can re-pack.
    ///
    /// Returns `false` when the atlas has already reached its maximum size.
    pub(crate) fn grow(&mut self) -> bool {
        if !self.packer.grow() {
            return false;
        }

        self.slots.clear();
        self.order.clear();
        self.uploads.clear();
        self.reserve_white();
        true
    }

    /// Forgets every glyph and discards queued uploads.
    ///
    /// The texture is not cleared: no mesh references the old glyphs any more, and new glyphs
    /// overwrite their own rectangles. This avoids uploading a full-size zeroed image.
    pub(crate) fn reset(&mut self) {
        self.packer.reset();
        self.slots.clear();
        self.order.clear();
        self.uploads.clear();
        self.reserve_white();
    }

    /// Re-reserves the white texel after the packer was cleared.
    fn reserve_white(&mut self) {
        let reserved = self.packer.reserve_top_left(WHITE_SIZE, WHITE_SIZE);
        debug_assert!(
            reserved.is_some(),
            "the atlas must be large enough for the white texel"
        );
    }

    /// Returns this frame's uploads and clears the pending state.
    pub(crate) fn take_textures(&mut self) -> TexturesDelta {
        let mut delta = TexturesDelta::default();
        let size = self.size();

        // Allocate the texture on first use, or replace it after the atlas grew. A system that
        // never produced a glyph does not allocate anything unless `ensure_texture` was called.
        let size_changed = self.uploaded_size != Some(size);
        let create = size_changed
            && (self.requested || !self.uploads.is_empty() || self.uploaded_size.is_some());
        self.requested = false;
        if create {
            self.uploaded_size = Some(size);
            delta.create.push((self.texture_id, size));
            // New textures are zero-initialized, so the reserved white texel has to be uploaded
            // whenever the texture is created or replaced after growth.
            delta.set.push((
                self.texture_id,
                ImageDelta::partial([0, 0], [WHITE_SIZE, WHITE_SIZE], WHITE_PIXEL.to_vec()),
            ));
        }

        delta.set.extend(
            self.uploads
                .drain(..)
                .map(|upload| (self.texture_id, upload)),
        );
        delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tinim_paint::pos2;

    fn glyph(width: u32, height: u32) -> RasterGlyph {
        RasterGlyph {
            width,
            height,
            left: 0.0,
            top: height as f32,
            color: false,
            pixels: vec![255; width as usize * height as usize * 4],
        }
    }

    fn key(glyph_id: u32) -> GlyphKey {
        GlyphKey {
            font: FontId(0),
            glyph_id,
            ppem: 16,
        }
    }

    fn is_white(delta: &ImageDelta) -> bool {
        delta.pos == Some([0, 0]) && delta.size == [1, 1] && delta.rgba == WHITE_PIXEL
    }

    #[test]
    fn glyph_uvs_cover_the_full_texel_footprint() {
        let mut atlas = Atlas::new(TextureId(1), 8, 8, 8);
        let slot = atlas.alloc(key(1), glyph(4, 4)).unwrap();

        // The white texel occupies `(0, 0)` plus one pixel of padding, so the first glyph starts
        // two pixels down. Its UVs still span exactly `width`/`height` texels: a quad spans
        // exactly that many physical pixels, and insetting by half a texel would blur the glyph.
        assert_eq!(slot.uv.min, pos2(0.0, 0.25));
        assert_eq!(slot.uv.max, pos2(0.5, 0.75));
    }

    #[test]
    fn glyphs_do_not_cover_the_white_texel() {
        let mut atlas = Atlas::new(TextureId(1), 8, 8, 8);
        let slot = atlas.alloc(key(1), glyph(4, 4)).unwrap();

        // The glyph rectangle starts at x >= 2 or y >= 2, leaving the white texel and its padding
        // untouched.
        assert!(slot.uv.min.x * 8.0 >= 2.0 || slot.uv.min.y * 8.0 >= 2.0);
    }

    #[test]
    fn ensure_texture_creates_the_atlas_and_uploads_white() {
        let mut atlas = Atlas::new(TextureId(1), 8, 8, 8);
        assert!(atlas.take_textures().is_empty(), "no glyph and no request");

        atlas.ensure_texture();
        let delta = atlas.take_textures();
        assert_eq!(delta.create, vec![(TextureId(1), [8, 8])]);
        assert_eq!(delta.set.len(), 1);
        assert_eq!(delta.set[0].0, TextureId(1));
        assert!(is_white(&delta.set[0].1));
        assert!(
            atlas.take_textures().is_empty(),
            "the texture already exists"
        );
    }

    #[test]
    fn create_uploads_white_before_glyphs() {
        let mut atlas = Atlas::new(TextureId(1), 8, 8, 8);
        atlas.alloc(key(1), glyph(4, 4)).unwrap();

        let delta = atlas.take_textures();
        assert_eq!(delta.create, vec![(TextureId(1), [8, 8])]);
        assert_eq!(delta.set.len(), 2);
        assert!(is_white(&delta.set[0].1));
        assert!(!is_white(&delta.set[1].1));
        assert_eq!(delta.set[1].1.pos, Some([0, 2]));
    }

    #[test]
    fn grow_re_reserves_and_reuploads_white() {
        let mut atlas = Atlas::new(TextureId(1), 8, 8, 16);
        atlas.alloc(key(1), glyph(4, 4)).unwrap();
        atlas.take_textures();

        assert!(atlas.grow());
        let slot = atlas.alloc(key(2), glyph(4, 4)).unwrap();
        assert!(slot.uv.min.y * 16.0 >= 2.0, "white stays at the top-left");

        let delta = atlas.take_textures();
        assert_eq!(delta.create, vec![(TextureId(1), [16, 16])]);
        assert!(delta.set.iter().any(|(_, image)| is_white(image)));
    }

    #[test]
    fn reset_re_reserves_the_white_texel() {
        let mut atlas = Atlas::new(TextureId(1), 8, 8, 8);
        let first = atlas.alloc(key(1), glyph(4, 4)).unwrap();
        atlas.take_textures();

        atlas.reset();
        let again = atlas.alloc(key(1), glyph(4, 4)).unwrap();
        assert_eq!(again.uv.min, first.uv.min);
        assert!(again.uv.min.y * 8.0 >= 2.0);
    }

    #[test]
    fn minimum_size_holds_the_white_texel() {
        let mut atlas = Atlas::new(TextureId(1), 1, 1, 1);
        atlas.ensure_texture();
        let delta = atlas.take_textures();
        assert_eq!(delta.create, vec![(TextureId(1), [2, 2])]);
        assert!(is_white(&delta.set[0].1));
    }
}
