//! Texture uploads for the renderer (images and glyph coverage).
//!
//! Pixel data is straight-alpha sRGB: RGB is sRGB-encoded color and alpha is coverage. The renderer
//! uploads it into an `Rgba8Unorm` texture, so the sampler returns the encoded values unchanged and
//! blending happens in encoded (gamma) space. Non-color data (masks, normals, lookup tables) does
//! not belong in this format.
//!
//! ## Backend contract
//!
//! * [`TexturesDelta::create`] allocates a zero-initialized texture. Creating an id that already
//!   exists replaces the texture, so every texel not covered by a subsequent [`ImageDelta`] is
//!   transparent black.
//! * Apply `create` before `set`. A partial [`ImageDelta`] (`pos = Some(..)`) requires the texture
//!   to exist, either from an earlier frame or from the same delta.
//! * [`TextureId::WHITE`] is not part of a delta: the renderer creates it up front with an opaque
//!   white texel at `(0, 0)`. A texture used as a painter's solid texture must reserve the same
//!   texel and upload it whenever it is created or replaced.

use crate::TextureId;

/// How a texture is filtered when it is sampled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureFilter {
    Nearest,
    Linear,
}

/// How texture coordinates outside `0..=1` are resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureAddressMode {
    ClampToEdge,
    Repeat,
    MirrorRepeat,
}

/// Sampling options that belong to one texture.
///
/// The defaults match the renderer's historical behavior: linear filtering and clamped edges.
/// Textures that should tile can use [`TextureAddressMode::Repeat`]. Note that the sampler only
/// controls how out-of-range *UVs* are sampled; existing geometry helpers clamp UVs to `0..=1`, so
/// tiling also needs a way to emit UVs outside that range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureOptions {
    pub mag_filter: TextureFilter,
    pub min_filter: TextureFilter,
    pub address_mode_u: TextureAddressMode,
    pub address_mode_v: TextureAddressMode,
}

impl Default for TextureOptions {
    fn default() -> Self {
        Self {
            mag_filter: TextureFilter::Linear,
            min_filter: TextureFilter::Linear,
            address_mode_u: TextureAddressMode::ClampToEdge,
            address_mode_v: TextureAddressMode::ClampToEdge,
        }
    }
}

/// The allocation parameters for one texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureDescriptor {
    /// Size in pixels.
    pub size: [u32; 2],
    pub options: TextureOptions,
}

impl TextureDescriptor {
    #[inline]
    pub fn new(size: [u32; 2]) -> Self {
        Self {
            size,
            options: TextureOptions::default(),
        }
    }

    #[inline]
    pub fn with_options(mut self, options: TextureOptions) -> Self {
        self.options = options;
        self
    }
}

impl From<[u32; 2]> for TextureDescriptor {
    fn from(size: [u32; 2]) -> Self {
        Self::new(size)
    }
}

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
    /// Textures to allocate before applying [`TexturesDelta::set`].
    ///
    /// New textures are zero-initialized. A partial update can therefore be uploaded without
    /// first sending a full-size image. Creating an id that already exists replaces it.
    pub create: Vec<(TextureId, TextureDescriptor)>,
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

    /// Merge another frame's changes into this one.
    ///
    /// `create` entries from both deltas are applied before any `set` entry, so concatenating
    /// independent producers (for example a glyph atlas and user image uploads) is safe as long
    /// as they use disjoint texture ids.
    #[inline]
    pub fn append(&mut self, mut other: TexturesDelta) {
        self.create.append(&mut other.create);
        self.set.append(&mut other.set);
        self.free.append(&mut other.free);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_concatenates_each_list() {
        let mut a = TexturesDelta {
            create: vec![(TextureId(1), TextureDescriptor::new([1, 1]))],
            set: vec![(TextureId(1), ImageDelta::full([1, 1], vec![1, 2, 3, 4]))],
            free: vec![TextureId(2)],
        };
        let b = TexturesDelta {
            create: vec![(TextureId(3), TextureDescriptor::new([2, 2]))],
            set: vec![(TextureId(3), ImageDelta::full([2, 2], vec![0; 16]))],
            free: vec![TextureId(4)],
        };

        a.append(b);

        assert_eq!(
            a.create,
            vec![
                (TextureId(1), TextureDescriptor::new([1, 1])),
                (TextureId(3), TextureDescriptor::new([2, 2])),
            ]
        );
        assert_eq!(a.set.len(), 2);
        assert_eq!(a.free, vec![TextureId(2), TextureId(4)]);
    }

    #[test]
    fn texture_options_default_to_linear_clamped() {
        let options = TextureOptions::default();
        assert_eq!(options.mag_filter, TextureFilter::Linear);
        assert_eq!(options.min_filter, TextureFilter::Linear);
        assert_eq!(options.address_mode_u, TextureAddressMode::ClampToEdge);
        assert_eq!(options.address_mode_v, TextureAddressMode::ClampToEdge);
    }
}
