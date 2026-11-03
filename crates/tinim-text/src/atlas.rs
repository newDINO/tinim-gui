//! A shelf-packed RGBA8 atlas for rasterized glyphs.

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

#[derive(Clone, Copy, Debug)]
struct PixelRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl PixelRect {
    fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);
        Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        }
    }
}

pub(crate) struct Atlas {
    texture_id: TextureId,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    shelves: Vec<Shelf>,
    next_y: u32,
    slots: HashMap<GlyphKey, GlyphSlot>,
    dirty: Option<PixelRect>,
    needs_full_upload: bool,
}

impl Atlas {
    pub(crate) fn new(texture_id: TextureId, width: u32, height: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            texture_id,
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 4],
            shelves: Vec::new(),
            next_y: 0,
            slots: HashMap::new(),
            dirty: None,
            needs_full_upload: true,
        }
    }

    pub(crate) fn texture_id(&self) -> TextureId {
        self.texture_id
    }

    pub(crate) fn get(&self, key: &GlyphKey) -> Option<GlyphSlot> {
        self.slots.get(key).copied()
    }

    /// Copies a glyph into the atlas.
    ///
    /// Returns `None` when there is no room left. The caller is expected to reset the atlas and
    /// try again.
    pub(crate) fn alloc(&mut self, key: GlyphKey, glyph: &RasterGlyph) -> Option<GlyphSlot> {
        if let Some(slot) = self.slots.get(&key) {
            return Some(*slot);
        }
        if glyph.width == 0 || glyph.height == 0 {
            return None;
        }
        if glyph.width + PADDING > self.width || glyph.height + PADDING > self.height {
            return None;
        }

        let position = self.find_position(glyph.width, glyph.height)?;
        let (x, y) = position;
        self.blit(x, y, glyph);

        let inverse_width = 1.0 / self.width as f32;
        let inverse_height = 1.0 / self.height as f32;
        let slot = GlyphSlot {
            uv: Rect::from_min_max(
                pos2(
                    (x as f32 + 0.5) * inverse_width,
                    (y as f32 + 0.5) * inverse_height,
                ),
                pos2(
                    (x + glyph.width) as f32 * inverse_width - 0.5 * inverse_width,
                    (y + glyph.height) as f32 * inverse_height - 0.5 * inverse_height,
                ),
            ),
            left: glyph.left,
            top: glyph.top,
            width: glyph.width as f32,
            height: glyph.height as f32,
            color: glyph.color,
        };
        self.slots.insert(key, slot);
        Some(slot)
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

    fn blit(&mut self, x: u32, y: u32, glyph: &RasterGlyph) {
        let row_bytes = glyph.width as usize * 4;
        debug_assert_eq!(glyph.pixels.len(), row_bytes * glyph.height as usize);
        for row in 0..glyph.height as usize {
            let src = &glyph.pixels[row * row_bytes..(row + 1) * row_bytes];
            let dst_offset = ((y as usize + row) * self.width as usize + x as usize) * 4;
            self.pixels[dst_offset..dst_offset + row_bytes].copy_from_slice(src);
        }

        let dirty = PixelRect {
            x,
            y,
            width: glyph.width,
            height: glyph.height,
        };
        self.dirty = Some(match self.dirty {
            Some(current) => current.union(dirty),
            None => dirty,
        });
    }

    /// Forgets every glyph and requests a full upload on the next [`Atlas::take_textures`].
    pub(crate) fn reset(&mut self) {
        self.pixels.fill(0);
        self.shelves.clear();
        self.next_y = 0;
        self.slots.clear();
        self.dirty = None;
        self.needs_full_upload = true;
    }

    /// Returns this frame's uploads and clears the dirty state.
    pub(crate) fn take_textures(&mut self) -> TexturesDelta {
        let mut delta = TexturesDelta::default();
        if self.needs_full_upload {
            self.needs_full_upload = false;
            self.dirty = None;
            delta.set.push((
                self.texture_id,
                ImageDelta::full([self.width, self.height], self.pixels.clone()),
            ));
        } else if let Some(dirty) = self.dirty.take() {
            delta.set.push((
                self.texture_id,
                ImageDelta::partial(
                    [dirty.x, dirty.y],
                    [dirty.width, dirty.height],
                    self.extract(dirty),
                ),
            ));
        }
        delta
    }

    fn extract(&self, rect: PixelRect) -> Vec<u8> {
        let row_bytes = rect.width as usize * 4;
        let mut out = Vec::with_capacity(row_bytes * rect.height as usize);
        for row in 0..rect.height as usize {
            let offset = ((rect.y as usize + row) * self.width as usize + rect.x as usize) * 4;
            out.extend_from_slice(&self.pixels[offset..offset + row_bytes]);
        }
        out
    }
}
