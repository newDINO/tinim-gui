//! User textures owned by a [`Context`](crate::Context).
//!
//! Decoding is intentionally not part of this crate. Produce tightly packed RGBA8 pixels (for
//! example with `png` or `image` in the application) and call
//! [`Context::load_texture`](crate::Context::load_texture). The pixels must be sRGB-encoded with
//! straight alpha; the renderer blends them in encoded space and does not expect premultiplied
//! colors.

use tinim_paint::{TextureId, Vec2};

/// Tightly packed RGBA8 pixels ready to be uploaded.
///
/// `rgba.len()` must equal `size[0] * size[1] * 4`. Colors are sRGB-encoded and alpha is straight
/// (not premultiplied), matching [`tinim_paint::ImageDelta`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageData {
    /// Size in pixels.
    pub size: [u32; 2],
    /// Tightly packed RGBA8 pixels.
    pub rgba: Vec<u8>,
}

impl ImageData {
    /// Creates image data, panicking if the pixel buffer length does not match `size`.
    pub fn new(size: [u32; 2], rgba: Vec<u8>) -> Self {
        assert_eq!(
            rgba.len(),
            size[0] as usize * size[1] as usize * 4,
            "ImageData: pixel buffer has the wrong length"
        );
        Self { size, rgba }
    }
}

/// A texture allocated by [`Context::load_texture`](crate::Context::load_texture).
///
/// The handle stores the texture id and its pixel dimensions. It is [`Copy`], so releasing the
/// texture is explicit: call [`Context::free_texture`](crate::Context::free_texture) when the
/// texture is no longer needed. Forgetting to free a texture only leaks it, which is usually
/// acceptable for a small number of long-lived images.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureHandle {
    id: TextureId,
    size: [u32; 2],
}

impl TextureHandle {
    pub(crate) fn new(id: TextureId, size: [u32; 2]) -> Self {
        Self { id, size }
    }

    /// The texture id used by paint commands.
    pub fn id(&self) -> TextureId {
        self.id
    }

    /// The texture size in pixels.
    pub fn pixel_size(&self) -> [u32; 2] {
        self.size
    }

    /// The natural size in logical points.
    ///
    /// Like an HTML `<img>` without explicit dimensions, the pixel width and height are used
    /// directly as logical units. At a scale factor above 1 the image is therefore upscaled by
    /// the renderer; pass an explicit size to [`Ui::image_with`](crate::Ui::image_with) when a
    /// different size is wanted.
    pub fn size(&self) -> Vec2 {
        Vec2::new(self.size[0] as f32, self.size[1] as f32)
    }
}
