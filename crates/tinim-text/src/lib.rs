//! Text shaping, line layout and a glyph atlas for `tinim-gui`.
//!
//! The crate is deliberately small: it uses [`harfrust`] for OpenType shaping and [`skrifa`] for
//! font metadata, outlines, bitmaps and COLRv0. Rasterization is done by [`zeno`] and the result is
//! packed into one RGBA8 texture that can be uploaded through [`tinim_paint::TexturesDelta`].
//!
//! ```
//! use tinim_paint::{Pos2, TextureId, pos2};
//! use tinim_text::{TextStyle, TextSystem};
//!
//! # fn build(font_data: Vec<u8>) {
//! let mut text = TextSystem::new(TextureId(1));
//! text.add_font(font_data).unwrap();
//!
//! let layout = text.layout("Hello", &TextStyle::default(), Some(200.0));
//! let mesh = text.prepare(&layout, pos2(10.0, 20.0), 1.0);
//! let textures = text.take_textures();
//! # let _ = (mesh, textures);
//! # }
//! ```
//!
//! Coordinates in [`TextLayout`] are logical points. [`TextSystem::prepare`] takes the current
//! `pixels_per_point` and emits a [`tinim_paint::Mesh`] in physical pixels, ready for
//! [`tinim_paint::Painter::add_mesh`].
//!
//! ## Fonts
//!
//! The first font added is the primary font. With the `fallback` feature (enabled by default) later
//! fonts are used for characters the primary font doesn't cover. Emoji clusters prefer a face with
//! a color representation.
//!
//! There is no bundled font. Use [`TextSystem::add_font`] with your own bytes, or enable the
//! `system-fonts` feature and call [`TextSystem::load_system_fonts`]. System font files are
//! memory-mapped instead of read into the heap, so the OS can page them in on demand.
//!
//! ## Emoji
//!
//! Bitmap strikes (CBDT/CBLC, EBDT/EBLC and sbix) and COLRv0 are supported. PNG-compressed strikes
//! need the `emoji-bitmap` feature, which is on by default. COLRv1 gradient graphs are not
//! implemented yet; those glyphs fall back to their monochrome outline when one exists.

mod atlas;
mod error;
mod font;
mod layout;
mod raster;
mod shape;
#[cfg(feature = "system-fonts")]
mod system;

use std::path::Path;

use skrifa::GlyphId;
#[cfg(feature = "system-fonts")]
use skrifa::raw::CollectionRef;
use tinim_paint::{Color32, Mesh, Pos2, Rect, TextureId, TexturesDelta, pos2, vec2};

pub use error::FontError;
pub use font::{FontFace, FontId, FontMetrics, FontStack};
pub use layout::{PositionedGlyph, TextDirection, TextLayout, TextLine, TextStyle};

use atlas::{Atlas, GlyphKey};

const DEFAULT_ATLAS_SIZE: u32 = 2048;

/// Glyphs larger than this are not useful in a default atlas and only waste memory.
const MAX_PPEM: f32 = DEFAULT_ATLAS_SIZE as f32;

/// Owns the font stack, the glyph atlas and the rasterization cache.
pub struct TextSystem {
    fonts: FontStack,
    atlas: Atlas,
}

impl TextSystem {
    /// Creates a text system whose glyph atlas is uploaded as `texture_id`.
    ///
    /// `TextureId::WHITE` is reserved by the renderer, so use another id.
    pub fn new(texture_id: TextureId) -> Self {
        Self::with_atlas_size(texture_id, DEFAULT_ATLAS_SIZE)
    }

    /// Creates a text system with a custom square atlas size.
    pub fn with_atlas_size(texture_id: TextureId, size: u32) -> Self {
        Self {
            fonts: FontStack::default(),
            atlas: Atlas::new(texture_id, size, size),
        }
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

    /// Rasterizes any new glyphs and builds a mesh in physical pixels.
    ///
    /// `origin` is the top-left corner of the text block in logical points. Upload
    /// [`TextSystem::take_textures`] before drawing the returned mesh.
    pub fn prepare(&mut self, layout: &TextLayout, origin: Pos2, pixels_per_point: f32) -> Mesh {
        let pixels_per_point = if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
            pixels_per_point
        } else {
            1.0
        };
        let ppem = (layout.style().font_size * pixels_per_point)
            .round()
            .clamp(1.0, MAX_PPEM) as u32;

        // If the atlas fills up we clear it and start over. Building the mesh only after every
        // glyph has been allocated makes that retry safe: no stale UVs are left behind.
        let mut cleared = false;
        loop {
            match self.try_prepare(layout, origin, pixels_per_point, ppem, cleared) {
                Some(mesh) => return mesh,
                None => {
                    self.atlas.reset();
                    cleared = true;
                }
            }
        }
    }

    fn try_prepare(
        &mut self,
        layout: &TextLayout,
        origin: Pos2,
        pixels_per_point: f32,
        ppem: u32,
        skip_overflow: bool,
    ) -> Option<Mesh> {
        let glyph_count: usize = layout.lines().iter().map(|line| line.glyphs().len()).sum();
        let mut slots = Vec::with_capacity(glyph_count);

        for line in layout.lines() {
            for glyph in line.glyphs() {
                let key = GlyphKey {
                    font: glyph.font,
                    glyph_id: glyph.glyph_id,
                    ppem,
                };
                if let Some(slot) = self.atlas.get(&key) {
                    slots.push(Some(slot));
                    continue;
                }
                let Some(face) = self.fonts.get(glyph.font) else {
                    slots.push(None);
                    continue;
                };
                let Some(raster) =
                    raster::rasterize(face, GlyphId::new(glyph.glyph_id), ppem as f32)
                else {
                    slots.push(None);
                    continue;
                };
                match self.atlas.alloc(key, raster) {
                    Some(slot) => slots.push(Some(slot)),
                    None if skip_overflow => slots.push(None),
                    None => return None,
                }
            }
        }

        let mut mesh = Mesh::with_texture(self.atlas.texture_id());
        let mut index = 0;
        for line in layout.lines() {
            let baseline = (origin.y + line.baseline()) * pixels_per_point;
            for glyph in line.glyphs() {
                if let Some(slot) = slots[index] {
                    let x = ((origin.x + glyph.x) * pixels_per_point + slot.left).round();
                    let y = (baseline - glyph.y * pixels_per_point - slot.top).round();
                    let rect = Rect::from_min_size(pos2(x, y), vec2(slot.width, slot.height));
                    let color = if slot.color {
                        Color32::from_rgba_unmultiplied(
                            255,
                            255,
                            255,
                            layout.style().color.to_array()[3],
                        )
                    } else {
                        layout.style().color
                    };
                    mesh.add_rect_with_uv(rect, slot.uv, color);
                }
                index += 1;
            }
        }

        Some(mesh)
    }

    /// Returns this frame's atlas uploads and clears the dirty state.
    pub fn take_textures(&mut self) -> TexturesDelta {
        self.atlas.take_textures()
    }

    /// The texture that must be bound when drawing meshes from [`TextSystem::prepare`].
    pub fn atlas_texture_id(&self) -> TextureId {
        self.atlas.texture_id()
    }

    /// Forgets every cached glyph.
    ///
    /// The GPU texture is left untouched; the old texels are simply no longer referenced. New
    /// glyphs are uploaded into their own rectangles as they are rasterized.
    pub fn reset_atlas(&mut self) {
        self.atlas.reset();
    }
}
