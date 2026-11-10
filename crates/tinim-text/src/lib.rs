//! Text shaping, line layout and glyph rasterization.
//!
//! The crate is deliberately small: it uses [`harfrust`] for OpenType shaping and [`skrifa`] for
//! font metadata, outlines, bitmaps and COLRv0. Rasterization is done by [`ab_glyph_rasterizer`]. Nothing here
//! knows about textures, atlases or meshes; a paint integration such as `tinim-paint-text` turns
//! [`TextLayout`]s and [`RasterizedGlyph`]s into backend-specific draw data.
//!
//! ```
//! use tinim_text::{TextContext, TextStyle};
//!
//! # fn build(font_data: Vec<u8>) {
//! let mut text = TextContext::new();
//! text.add_font(font_data).unwrap();
//!
//! let layout = text.layout("Hello", &TextStyle::default(), Some(200.0));
//! let glyph = &layout.lines()[0].glyphs()[0];
//! let bitmap = tinim_text::rasterize(
//!     text.font(glyph.font()).unwrap(),
//!     glyph.glyph_id(),
//!     layout.style().font_size(),
//! );
//! # let _ = (layout, bitmap);
//! # }
//! ```
//!
//! Coordinates in [`TextLayout`] are logical points. Rasterization takes a pixels-per-em value,
//! so callers multiply by their `pixels_per_point` themselves.
//!
//! ## Fonts
//!
//! The first font added is the primary font. With the `fallback` feature (enabled by default) later
//! fonts are used for characters the primary font doesn't cover. Emoji clusters prefer a face with
//! a color representation.
//!
//! There is no bundled font. Use [`TextContext::add_font`] with your own bytes, or enable the
//! `system-fonts` feature and call `TextContext::load_system_fonts`. System font files are
//! memory-mapped instead of read into the heap, so the OS can page them in on demand.
//!
//! ## Emoji
//!
//! Bitmap strikes (CBDT/CBLC, EBDT/EBLC and sbix) and COLRv0 are supported. PNG-compressed strikes
//! need the `emoji-bitmap` feature, which is on by default. COLRv1 gradient graphs are not
//! implemented yet; those glyphs fall back to their monochrome outline when one exists.

mod error;
mod font;
mod layout;
mod raster;
mod shape;
#[cfg(feature = "system-fonts")]
mod system;

use std::path::Path;

pub use glam::{Vec2, vec2};
#[cfg(feature = "system-fonts")]
use skrifa::raw::CollectionRef;

pub use error::FontError;
pub use font::{FontFace, FontId, FontMetrics, FontStack};
pub use layout::{
    LayoutKey, LayoutStyle, PositionedGlyph, TextDirection, TextLayout, TextLine, TextStyle,
};
pub use raster::{RasterizedGlyph, rasterize};

/// Owns the font stack and shapes text into layouts.
///
/// This is the paint-agnostic half of a text system. It does not rasterize anything eagerly;
/// call [`rasterize`] for the glyphs you actually want to draw, or let a paint integration cache
/// them.
#[derive(Debug, Default)]
pub struct TextContext {
    fonts: FontStack,
}

impl TextContext {
    /// Creates an empty text context with no fonts.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a font from memory. The first font added becomes the primary font.
    pub fn add_font(&mut self, data: impl Into<Vec<u8>>) -> Result<FontId, FontError> {
        self.add_font_at(data, 0)
    }

    /// Adds one face from a font collection.
    pub fn add_font_at(
        &mut self,
        data: impl Into<Vec<u8>>,
        index: u32,
    ) -> Result<FontId, FontError> {
        let face = FontFace::new(data.into(), index)?;
        Ok(self.fonts.push(face))
    }

    /// Reads and adds a font file. Collections add their first face only.
    pub fn add_font_file(&mut self, path: impl AsRef<Path>) -> Result<FontId, FontError> {
        let data = std::fs::read(path)?;
        self.add_font(data)
    }

    /// All currently loaded faces, in fallback order.
    pub fn fonts(&self) -> &FontStack {
        &self.fonts
    }

    pub fn font(&self, id: FontId) -> Option<&FontFace> {
        self.fonts.get(id)
    }

    /// Scans the usual system font directories and adds every face found.
    ///
    /// Returns the number of faces added. This is a no-op without the `system-fonts` feature.
    ///
    /// The first face added becomes the primary font, but filesystem discovery order is not
    /// meaningful. If the choice of primary face matters, add a preferred font first and use the
    /// system faces only for fallback.
    #[cfg(feature = "system-fonts")]
    pub fn load_system_fonts(&mut self) -> usize {
        let mut added = 0;
        for path in system::font_files() {
            let Ok(file) = std::fs::File::open(&path) else {
                continue;
            };
            match system::map_font_file(&file) {
                Ok(mmap) => added += self.add_font_mapped(std::sync::Arc::new(mmap)).len(),
                Err(_) => {
                    // Mapping can fail on exotic filesystems; read the file instead.
                    let Ok(data) = std::fs::read(&path) else {
                        continue;
                    };
                    added += self.add_font_bytes(data).len();
                }
            }
        }
        added
    }

    #[cfg(feature = "system-fonts")]
    fn add_font_mapped(&mut self, data: std::sync::Arc<memmap2::Mmap>) -> Vec<FontId> {
        // Faces of a collection share the same mapping instead of mapping the file once per face.
        if let Ok(collection) = CollectionRef::new(&data[..]) {
            let count = collection.len();
            let mut ids = Vec::new();
            for index in 0..count {
                if let Ok(face) = FontFace::from_mapped(data.clone(), index) {
                    ids.push(self.fonts.push(face));
                }
            }
            ids
        } else {
            match FontFace::from_mapped(data, 0) {
                Ok(face) => vec![self.fonts.push(face)],
                Err(_) => Vec::new(),
            }
        }
    }

    #[cfg(feature = "system-fonts")]
    fn add_font_bytes(&mut self, data: Vec<u8>) -> Vec<FontId> {
        // Faces of a collection share the same bytes instead of copying them once per face.
        let data: std::sync::Arc<[u8]> = data.into();
        if let Ok(collection) = CollectionRef::new(&data) {
            let count = collection.len();
            let mut ids = Vec::new();
            for index in 0..count {
                if let Ok(face) = FontFace::from_shared(data.clone(), index) {
                    ids.push(self.fonts.push(face));
                }
            }
            ids
        } else {
            match FontFace::from_shared(data, 0) {
                Ok(face) => vec![self.fonts.push(face)],
                Err(_) => Vec::new(),
            }
        }
    }

    /// Shapes and line-breaks `text`. Nothing is rasterized yet.
    pub fn layout(&self, text: &str, style: &TextStyle, max_width: Option<f32>) -> TextLayout {
        layout::layout(
            &self.fonts,
            text,
            style,
            max_width,
            cfg!(feature = "fallback"),
        )
    }
}
