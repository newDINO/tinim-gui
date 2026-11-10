//! Font storage, coverage checks and fallback selection.

use std::sync::Arc;

use skrifa::{
    FontRef, GlyphId, MetadataProvider,
    instance::{LocationRef, Size},
    raw::TableProvider,
};

use crate::error::FontError;

/// Identifies a face in a [`FontStack`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontId(pub(crate) u32);

impl FontId {
    /// Creates an id from an index in a [`FontStack`].
    ///
    /// Most callers get ids from [`crate::TextContext::add_font`] or [`FontStack::iter`]. This is
    /// mainly useful for caches and tests that need a stable key without a font stack at hand.
    pub const fn from_index(index: u32) -> Self {
        Self(index)
    }
}

/// The bytes behind a [`FontFace`].
///
/// User-provided fonts are owned, while system fonts stay memory-mapped so that the OS can page
/// them in on demand instead of keeping every installed font resident in the heap.
#[derive(Clone)]
enum FontData {
    Owned(Arc<[u8]>),
    #[cfg(feature = "system-fonts")]
    Mapped(Arc<memmap2::Mmap>),
}

impl FontData {
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Owned(data) => data,
            #[cfg(feature = "system-fonts")]
            Self::Mapped(data) => data,
        }
    }
}

/// A parsed font face, kept alive for as long as the [`crate::TextContext`] exists.
///
/// The font is parsed again on every use. `FontRef` is a cheap view over the bytes (it only
/// validates the table directory), so this avoids self-referential structs and any `unsafe`.
#[derive(Clone)]
pub struct FontFace {
    data: FontData,
    index: u32,
    units_per_em: u16,
    has_color_tables: bool,
    shaper_data: Arc<harfrust::ShaperData>,
}

impl std::fmt::Debug for FontFace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontFace")
            .field("index", &self.index)
            .field("units_per_em", &self.units_per_em)
            .finish_non_exhaustive()
    }
}

impl FontFace {
    pub(crate) fn new(data: Vec<u8>, index: u32) -> Result<Self, FontError> {
        Self::from_shared(data.into(), index)
    }

    /// Creates a face from bytes that may be shared with other faces of the same collection.
    pub(crate) fn from_shared(data: Arc<[u8]>, index: u32) -> Result<Self, FontError> {
        Self::from_data(FontData::Owned(data), index)
    }

    /// Creates a face backed by a memory-mapped system font file.
    #[cfg(feature = "system-fonts")]
    pub(crate) fn from_mapped(data: Arc<memmap2::Mmap>, index: u32) -> Result<Self, FontError> {
        Self::from_data(FontData::Mapped(data), index)
    }

    fn from_data(data: FontData, index: u32) -> Result<Self, FontError> {
        let font = FontRef::from_index(data.as_slice(), index).map_err(|err| match err {
            skrifa::raw::ReadError::InvalidCollectionIndex(index) => FontError::InvalidIndex(index),
            _ => FontError::InvalidFont,
        })?;
        let units_per_em = font
            .head()
            .map(|head| head.units_per_em())
            .unwrap_or(1000)
            .max(1);
        let tag = |bytes| skrifa::raw::types::Tag::new(bytes);
        let has_color_tables = font.table_data(tag(b"COLR")).is_some()
            || font.table_data(tag(b"CBDT")).is_some()
            || font.table_data(tag(b"CBLC")).is_some()
            || font.table_data(tag(b"EBDT")).is_some()
            || font.table_data(tag(b"EBLC")).is_some()
            || font.table_data(tag(b"sbix")).is_some();
        let shaper_data = Arc::new(harfrust::ShaperData::new(&font));
        Ok(Self {
            data,
            index,
            units_per_em,
            has_color_tables,
            shaper_data,
        })
    }

    /// Parses this face again. Cheap: the table directory is validated, no glyph data is read.
    pub(crate) fn font(&self) -> Result<FontRef<'_>, FontError> {
        FontRef::from_index(self.data.as_slice(), self.index).map_err(|_| FontError::InvalidFont)
    }

    /// Font design units per em.
    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// Whether the font's character map has an entry for `ch`.
    pub fn has_char(&self, ch: char) -> bool {
        self.font()
            .map(|font| font.charmap().map(ch).is_some())
            .unwrap_or(false)
    }

    pub(crate) fn glyph_for(&self, ch: char) -> Option<GlyphId> {
        self.font().ok()?.charmap().map(ch)
    }

    /// Whether `glyph_id` is painted with a color representation (bitmap or COLRv0).
    pub(crate) fn has_color_glyph(&self, glyph_id: GlyphId) -> bool {
        if !self.has_color_tables {
            return false;
        }
        let Ok(font) = self.font() else {
            return false;
        };
        if font
            .color_glyphs()
            .get_with_format(glyph_id, skrifa::color::ColorGlyphFormat::ColrV0)
            .is_some()
        {
            return true;
        }
        font.bitmap_strikes()
            .iter()
            .any(|strike| strike.get(glyph_id).is_some())
    }

    pub(crate) fn shaper_data(&self) -> &harfrust::ShaperData {
        &self.shaper_data
    }

    pub(crate) fn has_color_tables(&self) -> bool {
        self.has_color_tables
    }

    pub(crate) fn metrics(&self, size: f32) -> FontMetrics {
        let Ok(font) = self.font() else {
            return FontMetrics::fallback(size);
        };
        let metrics = font.metrics(Size::new(size), LocationRef::default());
        FontMetrics {
            ascent: metrics.ascent,
            // Skrifa reports the descent as a negative offset from the baseline (the same sign
            // as the font tables), while `FontMetrics` stores it as a positive distance.
            descent: -metrics.descent,
            leading: metrics.leading,
        }
    }
}

/// Vertical metrics of a face at a specific size, in the same units as the requested size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FontMetrics {
    /// Distance from the baseline to the top of the alignment box.
    pub ascent: f32,
    /// Distance from the baseline to the bottom of the alignment box.
    pub descent: f32,
    /// Recommended extra spacing between lines.
    pub leading: f32,
}

impl FontMetrics {
    pub(crate) fn fallback(size: f32) -> Self {
        Self {
            ascent: size * 0.8,
            descent: size * 0.2,
            leading: 0.0,
        }
    }

    pub(crate) fn line_height(self) -> f32 {
        (self.ascent + self.descent + self.leading).max(0.0)
    }
}

/// An ordered list of faces. The first face is the primary font.
#[derive(Clone, Debug, Default)]
pub struct FontStack {
    faces: Vec<FontFace>,
}

impl FontStack {
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    pub fn len(&self) -> usize {
        self.faces.len()
    }

    pub fn get(&self, id: FontId) -> Option<&FontFace> {
        self.faces.get(id.0 as usize)
    }

    pub fn iter(&self) -> impl Iterator<Item = (FontId, &FontFace)> {
        self.faces
            .iter()
            .enumerate()
            .map(|(index, face)| (FontId(index as u32), face))
    }

    pub(crate) fn push(&mut self, face: FontFace) -> FontId {
        let id = FontId(self.faces.len() as u32);
        self.faces.push(face);
        id
    }

    pub(crate) fn primary(&self) -> Option<(FontId, &FontFace)> {
        self.faces.first().map(|face| (FontId(0), face))
    }

    /// Picks a face for one grapheme cluster.
    ///
    /// The primary face wins whenever it covers the cluster. With the `fallback` feature (on by
    /// default) later faces are tried when it does not. Emoji clusters prefer the first face that
    /// has a color representation for them.
    pub(crate) fn select(&self, cluster: &str, allow_fallback: bool) -> Option<FontId> {
        let (primary_id, _) = self.primary()?;
        if !allow_fallback || self.faces.len() == 1 {
            return Some(primary_id);
        }

        let chars = cluster_chars(cluster);
        if chars.is_empty() {
            return Some(primary_id);
        }

        let prefer_color = cluster_prefers_color(cluster);
        let mut first_cover = None;
        for (index, face) in self.faces.iter().enumerate() {
            let covers_all = chars.iter().all(|&ch| face.has_char(ch));
            if !covers_all {
                continue;
            }
            let id = FontId(index as u32);
            if !prefer_color {
                // The stack is searched in fallback order, so the first covering face wins.
                return Some(id);
            }
            let is_color = face
                .glyph_for(chars[0])
                .map(|glyph| face.has_color_glyph(glyph))
                .unwrap_or(false);
            if is_color {
                return Some(id);
            }
            first_cover.get_or_insert(id);
        }

        Some(first_cover.unwrap_or(primary_id))
    }

    /// Whether a specific face covers every meaningful character in `cluster`.
    pub(crate) fn covers(&self, font: FontId, cluster: &str) -> bool {
        let Some(face) = self.get(font) else {
            return false;
        };
        if cluster.is_empty() {
            return false;
        }
        let mut meaningful = false;
        for ch in cluster.chars().filter(|ch| !is_ignorable(*ch)) {
            meaningful = true;
            if !face.has_char(ch) {
                return false;
            }
        }
        // All-ignorable clusters still need a face that maps them.
        meaningful || cluster.chars().all(|ch| face.has_char(ch))
    }
}

/// The characters a fallback face must cover for one grapheme cluster.
fn cluster_chars(cluster: &str) -> Vec<char> {
    let chars: Vec<char> = cluster.chars().filter(|ch| !is_ignorable(*ch)).collect();
    if chars.is_empty() {
        cluster.chars().collect()
    } else {
        chars
    }
}

/// Characters that should not by themselves prevent a font from being selected: variation
/// selectors, joiners, and other default-ignorables commonly found in emoji sequences.
fn is_ignorable(ch: char) -> bool {
    matches!(
        ch as u32,
        0x00AD
            | 0x034F
            | 0x061C
            | 0x17B4..=0x17B5
            | 0x180B..=0x180F
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x206F
            | 0xFE00..=0xFE0F
            | 0xFEFF
            | 0xE0020..=0xE007F
            | 0xE0100..=0xE01EF
    )
}

fn cluster_prefers_color(cluster: &str) -> bool {
    cluster.chars().any(|ch| ch == '\u{FE0F}') || cluster.chars().next().is_some_and(is_emoji_char)
}

/// A cheap approximation of the Unicode emoji properties, used only to prefer color faces.
pub(crate) fn is_emoji_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x231A..=0x23FF
    )
}
