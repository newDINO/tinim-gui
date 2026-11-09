//! Glyph atlas and mesh generation for [`tinim_text`] on top of [`tinim_paint`].
//!
//! [`tinim_text`] owns shaping, line layout and glyph rasterization. This crate caches the
//! rasterized glyphs in one shelf-packed RGBA8 atlas and turns a [`TextLayout`] into a
//! [`tinim_paint::Mesh`] that can be added to a painter.
//!
//! ```
//! use tinim_paint::{Color32, TextureId, pos2};
//! use tinim_paint_text::TextSystem;
//! use tinim_text::TextStyle;
//!
//! # fn build(font_data: Vec<u8>) {
//! let mut text = TextSystem::new(TextureId(1));
//! text.add_font(font_data).unwrap();
//!
//! let layout = text.layout("Hello", &TextStyle::default(), Some(200.0));
//! let mesh = text.prepare(&layout, pos2(10.0, 20.0), Color32::BLACK, 1.0);
//! let textures = text.take_textures();
//! # let _ = (mesh, textures);
//! # }
//! ```
//!
//! Coordinates in [`TextLayout`] are logical points. [`TextSystem::prepare`] takes the current
//! `pixels_per_point` and emits a [`tinim_paint::Mesh`] in physical pixels. The mesh is tagged as
//! [`tinim_paint::MeshSpace::Physical`], so [`tinim_paint::Painter::add_mesh`] appends it
//! unchanged instead of scaling it again.
//!
//! Upload [`TextSystem::take_textures`] before drawing the mesh. The atlas reserves an opaque
//! white texel at `(0, 0)`, so the same texture can also be used as the painter's solid texture
//! (see [`TextSystem::ensure_atlas_texture`]).

mod atlas;
mod hash;
mod packer;

use std::path::Path;

use tinim_paint::{Color32, Mesh, Pos2, Rect, TextureId, TexturesDelta, pos2, vec2};
use tinim_text::{
    FontError, FontFace, FontId, FontStack, TextContext, TextLayout, TextStyle, rasterize,
};

use atlas::{Atlas, GlyphKey};

/// The atlas starts small and doubles when it fills up.
const INITIAL_ATLAS_SIZE: u32 = 512;

/// Upper bound for automatic growth. `with_atlas_size` can start larger than this.
const MAX_ATLAS_SIZE: u32 = 2048;

/// Glyphs larger than this are not useful in a default atlas and only waste memory.
const MAX_PPEM: f32 = MAX_ATLAS_SIZE as f32;

/// What to do to the atlas at the start of the next frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AtlasAction {
    Grow,
    Reset,
}

/// Owns the font stack, the glyph atlas and the rasterization cache.
///
/// This is the paint-facing facade over [`TextContext`]. Use [`TextSystem::text_context`] to
/// reach the shaping and rasterization API without going through the atlas.
pub struct TextSystem {
    text: TextContext,
    atlas: Atlas,
    /// Whether a mesh has been built since the last [`TextSystem::take_textures`].
    ///
    /// Growing or clearing the atlas re-packs every glyph, so it must not happen after a mesh
    /// from the same frame has been built. If the atlas overflows in that situation the frame
    /// skips the glyphs that do not fit and the action is deferred to the next frame.
    prepared_since_take: bool,
    pending_atlas: Option<AtlasAction>,
}

impl TextSystem {
    /// Creates a text system whose glyph atlas is uploaded as `texture_id`.
    ///
    /// The atlas starts small and grows on demand. It reserves an opaque white texel at `(0, 0)`,
    /// so the same texture can also be used as the painter's solid texture (see
    /// [`TextSystem::ensure_atlas_texture`]). `TextureId::WHITE` is reserved by the renderer, so
    /// use another id.
    pub fn new(texture_id: TextureId) -> Self {
        Self::with_atlas_size(texture_id, INITIAL_ATLAS_SIZE)
    }

    /// Creates a text system with a custom initial square atlas size.
    ///
    /// The atlas doubles its dimensions when it fills up, up to the built-in maximum (or the
    /// given size, if that is larger). Sizes are raised to at least 2×2 so the reserved white
    /// texel and its padding always fit.
    pub fn with_atlas_size(texture_id: TextureId, size: u32) -> Self {
        let max_size = MAX_ATLAS_SIZE.max(size);
        Self {
            text: TextContext::new(),
            atlas: Atlas::new(texture_id, size, size, max_size),
            prepared_since_take: false,
            pending_atlas: None,
        }
    }

    /// Adds a font from memory. The first font added becomes the primary font.
    pub fn add_font(&mut self, data: impl Into<Vec<u8>>) -> Result<FontId, FontError> {
        self.text.add_font(data)
    }

    /// Adds one face from a font collection.
    pub fn add_font_at(
        &mut self,
        data: impl Into<Vec<u8>>,
        index: u32,
    ) -> Result<FontId, FontError> {
        self.text.add_font_at(data, index)
    }

    /// Reads and adds a font file. Collections add their first face only.
    pub fn add_font_file(&mut self, path: impl AsRef<Path>) -> Result<FontId, FontError> {
        self.text.add_font_file(path)
    }

    /// All currently loaded faces, in fallback order.
    pub fn fonts(&self) -> &FontStack {
        self.text.fonts()
    }

    pub fn font(&self, id: FontId) -> Option<&FontFace> {
        self.text.font(id)
    }

    /// Scans the usual system font directories and adds every face found.
    ///
    /// Returns the number of faces added. This requires the `system-fonts` feature.
    #[cfg(feature = "system-fonts")]
    pub fn load_system_fonts(&mut self) -> usize {
        self.text.load_system_fonts()
    }

    /// The paint-agnostic text context: fonts, layout and rasterization.
    pub fn text_context(&self) -> &TextContext {
        &self.text
    }

    /// The paint-agnostic text context, mutably.
    pub fn text_context_mut(&mut self) -> &mut TextContext {
        &mut self.text
    }

    /// Shapes and line-breaks `text`. Nothing is rasterized yet.
    pub fn layout(&self, text: &str, style: &TextStyle, max_width: Option<f32>) -> TextLayout {
        self.text.layout(text, style, max_width)
    }

    /// Rasterizes any new glyphs and builds a mesh in physical pixels.
    ///
    /// `origin` is the top-left corner of the text block in logical points. `color` is a paint
    /// parameter and is not part of the cached layout. The mesh is tagged
    /// [`tinim_paint::MeshSpace::Physical`], so [`tinim_paint::Painter::add_mesh`] appends it
    /// without scaling it again. Upload [`TextSystem::take_textures`] before drawing the mesh.
    pub fn prepare(
        &mut self,
        layout: &TextLayout,
        origin: Pos2,
        color: Color32,
        pixels_per_point: f32,
    ) -> Mesh {
        let pixels_per_point = if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
            pixels_per_point
        } else {
            1.0
        };
        let ppem = (layout.style().font_size() * pixels_per_point)
            .round()
            .clamp(1.0, MAX_PPEM) as u32;

        self.apply_pending_atlas_action();

        // If the atlas overflows there are two cases:
        //
        // * No mesh has been built since the last take: re-packing the atlas is safe, so grow it
        //   (or clear it once it is at the maximum size) and retry.
        // * A mesh already references the current atlas: moving glyphs would invalidate that
        //   mesh. Defer the action and draw only the glyphs that fit in this frame.
        //
        // Building the mesh only after every glyph has been allocated makes both retries safe:
        // no stale UVs are left behind.
        let can_repack = !self.prepared_since_take;
        let mut allow_overflow = false;
        loop {
            if let Some(mesh) = self.try_prepare(
                layout,
                origin,
                color,
                pixels_per_point,
                ppem,
                allow_overflow,
            ) {
                self.prepared_since_take = true;
                return mesh;
            }

            if !can_repack {
                self.pending_atlas = Some(if self.atlas.can_grow() {
                    AtlasAction::Grow
                } else {
                    AtlasAction::Reset
                });
                allow_overflow = true;
                continue;
            }

            // Growth keeps the cached glyphs; clearing is the fallback when they can never all
            // fit in one texture.
            if !allow_overflow && self.grow_atlas() {
                continue;
            }
            self.atlas.reset();
            allow_overflow = true;
        }
    }

    /// Applies a growth or reset that was deferred from the previous frame.
    fn apply_pending_atlas_action(&mut self) {
        if self.prepared_since_take {
            return;
        }
        match self.pending_atlas.take() {
            Some(AtlasAction::Grow) => {
                if !self.grow_atlas() {
                    self.atlas.reset();
                }
            }
            Some(AtlasAction::Reset) => self.atlas.reset(),
            None => {}
        }
    }

    /// Grows the atlas and re-packs every cached glyph.
    ///
    /// Returns `false` if the atlas is already at its maximum size or the cached glyphs still do
    /// not fit after growing.
    fn grow_atlas(&mut self) -> bool {
        let keys = self.atlas.keys();
        loop {
            if !self.atlas.grow() {
                return false;
            }

            let mut all_fit = true;
            for key in &keys {
                let raster = {
                    let Some(face) = self.text.font(key.font) else {
                        continue;
                    };
                    rasterize(face, key.glyph_id, key.ppem as f32)
                };
                let Some(raster) = raster else {
                    continue;
                };
                if self.atlas.alloc(*key, raster).is_none() {
                    all_fit = false;
                    break;
                }
            }
            if all_fit {
                return true;
            }
        }
    }

    fn try_prepare(
        &mut self,
        layout: &TextLayout,
        origin: Pos2,
        color: Color32,
        pixels_per_point: f32,
        ppem: u32,
        skip_overflow: bool,
    ) -> Option<Mesh> {
        let glyph_count: usize = layout.lines().iter().map(|line| line.glyphs().len()).sum();
        let mut slots = Vec::with_capacity(glyph_count);

        for line in layout.lines() {
            for glyph in line.glyphs() {
                let key = GlyphKey {
                    font: glyph.font(),
                    glyph_id: glyph.glyph_id(),
                    ppem,
                };
                if let Some(slot) = self.atlas.get(&key) {
                    slots.push(Some(slot));
                    continue;
                }
                let Some(face) = self.text.font(glyph.font()) else {
                    slots.push(None);
                    continue;
                };
                let Some(raster) = rasterize(face, glyph.glyph_id(), ppem as f32) else {
                    slots.push(None);
                    continue;
                };
                // Empty glyphs (spaces, missing outlines) never need atlas space and must not
                // trigger growth.
                if raster.width == 0 || raster.height == 0 {
                    slots.push(None);
                    continue;
                }
                if !self.atlas.can_ever_fit(raster.width, raster.height) {
                    slots.push(None);
                    continue;
                }
                match self.atlas.alloc(key, raster) {
                    Some(slot) => slots.push(Some(slot)),
                    None if skip_overflow => slots.push(None),
                    None => return None,
                }
            }
        }

        let mut mesh = Mesh::physical(self.atlas.texture_id());
        let mut index = 0;
        for line in layout.lines() {
            let baseline = (origin.y + line.baseline()) * pixels_per_point;
            for glyph in line.glyphs() {
                if let Some(slot) = slots[index] {
                    let x = ((origin.x + glyph.x()) * pixels_per_point + slot.left).round();
                    let y = (baseline - glyph.y_offset() * pixels_per_point - slot.top).round();
                    let rect = Rect::from_min_size(pos2(x, y), vec2(slot.width, slot.height));
                    let color = if slot.color {
                        Color32::from_rgba_unmultiplied(255, 255, 255, color.to_array()[3])
                    } else {
                        color
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
        self.prepared_since_take = false;
        self.atlas.take_textures()
    }

    /// The texture that must be bound when drawing meshes from [`TextSystem::prepare`].
    pub fn atlas_texture_id(&self) -> TextureId {
        self.atlas.texture_id()
    }

    /// The current size of the glyph atlas, in pixels.
    pub fn atlas_size(&self) -> [u32; 2] {
        self.atlas.size()
    }

    /// Builds a mesh that draws the whole glyph atlas into `rect`.
    ///
    /// `rect` is in logical points and the returned mesh is a logical-space mesh, so
    /// [`tinim_paint::Painter::add_mesh`] scales it by `pixels_per_point`. This is mainly useful
    /// for inspecting atlas packing and the reserved white texel. Upload the atlas with
    /// [`TextSystem::take_textures`] before drawing the mesh; if no glyph has been rasterized
    /// yet, call [`TextSystem::ensure_atlas_texture`] first.
    pub fn atlas_mesh(&self, rect: Rect, color: Color32) -> Mesh {
        let mut mesh = Mesh::with_texture(self.atlas.texture_id());
        mesh.add_rect_with_uv(
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            color,
        );
        mesh
    }

    /// Ensures the atlas texture is allocated on the next [`TextSystem::take_textures`], even if
    /// no glyph has been rasterized yet.
    ///
    /// This is useful when the atlas is also used as a painter's solid texture: untextured
    /// geometry samples the opaque white texel reserved at `(0, 0)`, so the texture must exist
    /// before the first draw. The atlas always uploads that white texel whenever it creates or
    /// replaces the texture.
    pub fn ensure_atlas_texture(&mut self) {
        self.atlas.ensure_texture();
    }

    /// Forgets every cached glyph.
    ///
    /// The GPU texture is left untouched; the old texels are simply no longer referenced. New
    /// glyphs are uploaded into their own rectangles as they are rasterized.
    pub fn reset_atlas(&mut self) {
        self.atlas.reset();
        self.prepared_since_take = false;
        self.pending_atlas = None;
    }
}

/// Re-export the text crate so paint users can reach [`TextStyle`] without a second dependency.
pub use tinim_text;
