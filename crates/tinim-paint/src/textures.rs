//! Texture uploads for the renderer (images and glyph coverage).
//!
//! Pixel data is straight-alpha sRGB: RGB is sRGB color and alpha is linear coverage. The renderer
//! uploads it into an sRGB texture, so the sampler converts RGB to linear space. Non-color data
//! (masks, normals, lookup tables) does not belong in this format.

use crate::TextureId;

/// A full or partial RGBA8 texture update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageDelta {
    /// `None` means "create/replace the whole texture"; `Some(pos)` is a partial update.
    pub pos: Option<[u32; 2]>,
    /// Size in pixels of the uploaded region.
    pub size: [u32; 2],
    /// Tightly packed RGBA8 pixels (`size[0] * size[1] * 4` bytes), straight-alpha sRGB.
    pub rgba: Vec<u8>,
}

impl ImageDelta {
    pub fn full(size: [u32; 2], rgba: Vec<u8>) -> Self {
        assert_eq!(
            rgba.len(),
            size[0] as usize * size[1] as usize * 4,
            "ImageDelta::full: pixel buffer has the wrong length"
        );
        Self {
            pos: None,
            size,
            rgba,
        }
    }

    pub fn partial(pos: [u32; 2], size: [u32; 2], rgba: Vec<u8>) -> Self {
        assert_eq!(
            rgba.len(),
            size[0] as usize * size[1] as usize * 4,
            "ImageDelta::partial: pixel buffer has the wrong length"
        );
        Self {
            pos: Some(pos),
            size,
            rgba,
        }
    }
}

/// All texture changes of one frame, as produced by tessellation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TexturesDelta {
    /// Textures to allocate before applying [`TexturesDelta::set`], with their size in pixels.
    ///
    /// New textures are zero-initialized. A partial update can therefore be uploaded without
    /// first sending a full-size image.
    pub create: Vec<(TextureId, [u32; 2])>,
    pub set: Vec<(TextureId, ImageDelta)>,
    pub free: Vec<TextureId>,
}

impl TexturesDelta {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.create.is_empty() && self.set.is_empty() && self.free.is_empty()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.create.clear();
        self.set.clear();
        self.free.clear();
    }
}
