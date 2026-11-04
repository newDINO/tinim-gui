//! A shelf-packed RGBA8 atlas that uploads glyphs as individual texture patches.

use std::collections::HashMap;

use tinim_paint::{ImageDelta, Rect, TextureId, TexturesDelta, pos2};

use crate::font::FontId;

const PADDING: u32 = 1;

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
    /// Normalized texture coordinates, inset by half a texel to avoid bleeding.
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

#[derive(Clone, Copy, Debug)]
struct Shelf {
    y: u32,
    height: u32,
    x: u32,
}

pub(crate) struct Atlas {
    texture_id: TextureId,
    width: u32,
    height: u32,
    /// Largest square the atlas may grow to.
    max_size: u32,
    shelves: Vec<Shelf>,
    next_y: u32,
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
}

impl Atlas {
    pub(crate) fn new(texture_id: TextureId, width: u32, height: u32, max_size: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let max_size = max_size.max(width).max(height);
        Self {
            texture_id,
            width,
            height,
            max_size,
            shelves: Vec::new(),
            next_y: 0,
            slots: HashMap::new(),
            order: Vec::new(),
            uploads: Vec::new(),
            uploaded_size: None,
        }
    }

    pub(crate) fn texture_id(&self) -> TextureId {
        self.texture_id
    }

    pub(crate) fn get(&self, key: &GlyphKey) -> Option<GlyphSlot> {
        self.slots.get(key).copied()
    }

    /// Whether a glyph of this size could fit even at the maximum atlas size.
    pub(crate) fn can_ever_fit(&self, width: u32, height: u32) -> bool {
        width + PADDING <= self.max_size && height + PADDING <= self.max_size
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
        if width + PADDING > self.width || height + PADDING > self.height {
            return None;
        }

        let (x, y) = self.find_position(width, height)?;
        self.uploads
            .push(ImageDelta::partial([x, y], [width, height], pixels));

        let inverse_width = 1.0 / self.width as f32;
        let inverse_height = 1.0 / self.height as f32;
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
        let current = self.width.max(self.height);
        current.saturating_mul(2).min(self.max_size) > current
    }

    /// Doubles the atlas dimensions and forgets every slot so the caller can re-pack.
    ///
    /// Returns `false` when the atlas has already reached its maximum size.
    pub(crate) fn grow(&mut self) -> bool {
        let current = self.width.max(self.height);
        let next = current.saturating_mul(2).min(self.max_size);
        if next <= current {
            return false;
        }

        self.width = next;
        self.height = next;
        self.shelves.clear();
        self.next_y = 0;
        self.slots.clear();
        self.order.clear();
        self.uploads.clear();
        true
    }

    fn find_position(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        for shelf in &mut self.shelves {
            if shelf.height >= height && shelf.x + width + PADDING <= self.width {
                let position = (shelf.x, shelf.y);
                shelf.x += width + PADDING;
                return Some(position);
            }
        }

        let y = self.next_y;
        if y + height + PADDING > self.height {
            return None;
        }
        self.shelves.push(Shelf {
            y,
            height,
            x: width + PADDING,
        });
        self.next_y = y + height + PADDING;
        Some((0, y))
    }

    /// Forgets every glyph and discards queued uploads.
    ///
    /// The texture is not cleared: no mesh references the old glyphs any more, and new glyphs
    /// overwrite their own rectangles. This avoids uploading a full-size zeroed image.
    pub(crate) fn reset(&mut self) {
        self.shelves.clear();
        self.next_y = 0;
        self.slots.clear();
        self.order.clear();
        self.uploads.clear();
    }

    /// Returns this frame's uploads and clears the pending state.
    pub(crate) fn take_textures(&mut self) -> TexturesDelta {
        let mut delta = TexturesDelta::default();
        let size = [self.width, self.height];

        // Allocate the texture on first use, or replace it after the atlas grew. A system that
        // never produced a glyph does not allocate anything, even though no texture exists yet.
        let size_changed = self.uploaded_size != Some(size);
        if size_changed && (!self.uploads.is_empty() || self.uploaded_size.is_some()) {
            self.uploaded_size = Some(size);
            delta.create.push((self.texture_id, size));
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

    #[test]
    fn glyph_uvs_cover_the_full_texel_footprint() {
        let mut atlas = Atlas::new(TextureId(1), 8, 8, 8);
        let key = GlyphKey {
            font: FontId(0),
            glyph_id: 1,
            ppem: 16,
        };
        let slot = atlas.alloc(key, glyph(4, 4)).unwrap();

        // A quad spans exactly `width` physical pixels, so its UVs must span exactly `width`
        // texels. Insetting them by half a texel makes bilinear sampling blur the glyph.
        assert_eq!(slot.uv.min, pos2(0.0, 0.0));
        assert_eq!(slot.uv.max, pos2(0.5, 0.5));
    }
}
