//! Texture uploads produced while tessellating (the glyph atlas lives here).

use crate::TextureId;

/// A full or partial RGBA8 texture update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageDelta {
    /// `None` means "create/replace the whole texture"; `Some(pos)` is a partial update.
    pub pos: Option<[u32; 2]>,
    /// Size in pixels of the uploaded region.
    pub size: [u32; 2],
    /// Tightly packed RGBA8 pixels (`size[0] * size[1] * 4` bytes).
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
    pub set: Vec<(TextureId, ImageDelta)>,
    pub free: Vec<TextureId>,
}

impl TexturesDelta {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.set.is_empty() && self.free.is_empty()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.set.clear();
        self.free.clear();
    }
}
