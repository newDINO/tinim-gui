//! Font storage, coverage checks and fallback selection.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[cfg(any(feature = "system-fonts", test))]
use skrifa::raw::CollectionRef;
use skrifa::{
    Font,
    GlyphId,
    font::{
        attribute::Style as SkrifaStyle,
        glyph::{bitmap::BitmapSource, color::ColorVersion},
        name::NameId,
    },
    // The high-level font model deliberately does not report why a face failed to load, nor
    // enumerate the faces of a collection. Both need the underlying read-fonts view, which
    // skrifa re-exports as `raw`.
    raw::FileRef,
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

/// A font weight on the CSS 1–1000 scale.
///
/// The default is [`FontWeight::NORMAL`] (400). Values passed to [`FontWeight::new`] are clamped
/// to the valid range; zero is treated as normal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontWeight(u16);

impl FontWeight {
    pub const THIN: Self = Self(100);
    pub const EXTRA_LIGHT: Self = Self(200);
    pub const LIGHT: Self = Self(300);
    pub const NORMAL: Self = Self(400);
    pub const MEDIUM: Self = Self(500);
    pub const SEMI_BOLD: Self = Self(600);
    pub const BOLD: Self = Self(700);
    pub const EXTRA_BOLD: Self = Self(800);
    pub const BLACK: Self = Self(900);

    /// Creates a weight, clamping it to the CSS range of 1–1000.
    ///
    /// Zero is treated as [`FontWeight::NORMAL`] because it usually means "unset".
    pub const fn new(weight: u16) -> Self {
        Self(if weight == 0 {
            400
        } else if weight > 1000 {
            1000
        } else {
            weight
        })
    }

    /// The weight value.
    pub const fn value(self) -> u16 {
        self.0
    }
}

impl Default for FontWeight {
    fn default() -> Self {
        Self::NORMAL
    }
}

/// The slope of a face.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontStyle {
    /// An upright face.
    #[default]
    Normal,
    /// A cursive slanted face.
    Italic,
    /// A slanted face that keeps the upright letterforms.
    Oblique,
}

/// A font width on the CSS scale, expressed as a percentage.
///
/// The default is 100% ([`FontStretch::NORMAL`]). Values passed to [`FontStretch::new`] are
/// clamped to 1–1000; zero is treated as normal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontStretch(u16);

impl FontStretch {
    pub const ULTRA_CONDENSED: Self = Self(50);
    pub const EXTRA_CONDENSED: Self = Self(63);
    pub const CONDENSED: Self = Self(75);
    pub const SEMI_CONDENSED: Self = Self(88);
    pub const NORMAL: Self = Self(100);
    pub const SEMI_EXPANDED: Self = Self(113);
    pub const EXPANDED: Self = Self(125);
    pub const EXTRA_EXPANDED: Self = Self(150);
    pub const ULTRA_EXPANDED: Self = Self(200);

    /// Creates a stretch from a percentage, clamping it to 1–1000.
    ///
    /// Zero is treated as [`FontStretch::NORMAL`] because it usually means "unset".
    pub const fn new(percentage: u16) -> Self {
        Self(if percentage == 0 {
            100
        } else if percentage > 1000 {
            1000
        } else {
            percentage
        })
    }

    /// The stretch percentage.
    pub const fn value(self) -> u16 {
        self.0
    }
}

impl Default for FontStretch {
    fn default() -> Self {
        Self::NORMAL
    }
}

/// An ordered list of user-preferred font families, like CSS `font-family`.
///
/// Names are matched case-insensitively against the family names declared by loaded faces,
/// including localized names. A name that does not match any loaded face is ignored, and generic
/// CSS names such as `sans-serif` are not interpreted; use a concrete family name instead. When
/// the list is empty, the font stack's load order is used.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontFamilies(Arc<[Box<str>]>);

impl FontFamilies {
    /// Creates a family list from names.
    ///
    /// ```
    /// use tinim_text::FontFamilies;
    /// let families = FontFamilies::new(["Inter", "Noto Sans", "sans-serif"]);
    /// assert_eq!(families.len(), 3);
    /// ```
    pub fn new<I, S>(families: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self(
            families
                .into_iter()
                .map(|family| family.as_ref().into())
                .collect(),
        )
    }

    /// The family names, in preference order.
    pub fn as_slice(&self) -> &[Box<str>] {
        &self.0
    }

    /// The number of preferred families.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether no preferred families are set.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterates over the family names in preference order.
    pub fn iter(&self) -> std::slice::Iter<'_, Box<str>> {
        self.0.iter()
    }
}

impl From<&str> for FontFamilies {
    fn from(family: &str) -> Self {
        Self::new([family])
    }
}

impl From<String> for FontFamilies {
    fn from(family: String) -> Self {
        Self::new([family])
    }
}

impl From<Vec<String>> for FontFamilies {
    fn from(families: Vec<String>) -> Self {
        Self::new(families)
    }
}

impl<'a> IntoIterator for &'a FontFamilies {
    type Item = &'a Box<str>;
    type IntoIter = std::slice::Iter<'a, Box<str>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
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

    /// Shares the bytes with a fontations [`harfrust::Font`], which takes ownership of its source.
    fn to_source(&self) -> harfrust::font::Source {
        let shared: Arc<dyn AsRef<[u8]> + Send + Sync> = match self {
            Self::Owned(data) => Arc::new(SharedBytes(data.clone())),
            #[cfg(feature = "system-fonts")]
            Self::Mapped(data) => data.clone(),
        };
        shared.into()
    }
}

/// A sized wrapper so an `Arc<[u8]>` can be turned into a shared fontations source.
#[derive(Clone)]
struct SharedBytes(Arc<[u8]>);

impl AsRef<[u8]> for SharedBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// A parsed font face, kept alive for as long as the [`crate::TextContext`] exists.
///
/// The fontations font model owns a shared view of the bytes and caches the tables it reads, so
/// metadata, outlines, bitmaps and shaping all work from this one value without reparsing.
#[derive(Clone)]
pub struct FontFace {
    index: u32,
    units_per_em: u16,
    /// Whether the font has a table that can paint glyphs in color: COLR, CBDT/CBLC or sbix.
    has_color_tables: bool,
    /// Whether the font has any embedded bitmap table: CBDT/CBLC, EBDT/EBLC or sbix.
    ///
    /// Monochrome and grayscale EBDT strikes are coverage glyphs, not color glyphs.
    has_bitmap_tables: bool,
    /// Fontations font model. It also owns the cached layout data that harfrust attaches to the
    /// font, so it must outlive every [`harfrust::ShaperFont`].
    font: Font,
    family: Box<str>,
    family_key: Box<str>,
    family_names: Box<[Box<str>]>,
    full_name: Box<str>,
    weight: FontWeight,
    style: FontStyle,
    stretch: FontStretch,
}

impl std::fmt::Debug for FontFace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontFace")
            .field("family", &self.family)
            .field("weight", &self.weight)
            .field("style", &self.style)
            .field("stretch", &self.stretch)
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

    /// Creates every face in `data`, sharing the same byte buffer between them.
    ///
    /// A single-face file yields one face. Files that are not recognised as a collection are
    /// tried at index 0, just like a collection is read face by face.
    #[cfg(any(feature = "system-fonts", test))]
    pub(crate) fn from_shared_collection(data: Arc<[u8]>) -> Vec<Self> {
        match CollectionRef::new(&data) {
            Ok(collection) => (0..collection.len())
                .filter_map(|index| Self::from_shared(data.clone(), index).ok())
                .collect(),
            Err(_) => Self::from_shared(data, 0).ok().into_iter().collect(),
        }
    }

    /// Creates every face in a memory-mapped file, sharing the mapping between them.
    #[cfg(feature = "system-fonts")]
    pub(crate) fn from_mapped_collection(data: Arc<memmap2::Mmap>) -> Vec<Self> {
        match CollectionRef::new(&data[..]) {
            Ok(collection) => (0..collection.len())
                .filter_map(|index| Self::from_mapped(data.clone(), index).ok())
                .collect(),
            Err(_) => Self::from_mapped(data, 0).ok().into_iter().collect(),
        }
    }

    fn from_data(data: FontData, index: u32) -> Result<Self, FontError> {
        let font = Font::new(data.to_source(), index).ok_or_else(|| {
            // `Font::new` only reports success. Classify the failure with the raw file view so a
            // bad collection index keeps its dedicated error.
            match FileRef::new(data.as_slice()) {
                Ok(FileRef::Font(_)) if index != 0 => FontError::InvalidIndex(index),
                Ok(FileRef::Collection(collection)) if index >= collection.len() => {
                    FontError::InvalidIndex(index)
                }
                _ => FontError::InvalidFont,
            }
        })?;
        let units_per_em = font.units_per_em().max(1);
        let tables = font.tables();
        let has_color_tables = tables.colr_data().is_some()
            || tables.cbdt_data().is_some()
            || tables.cblc_data().is_some()
            || tables.sbix_data().is_some();
        let has_bitmap_tables = tables.cbdt_data().is_some()
            || tables.cblc_data().is_some()
            || tables.ebdt_data().is_some()
            || tables.eblc_data().is_some()
            || tables.sbix_data().is_some();
        let (family, family_names, full_name) = read_font_names(&font);
        let attributes = font.default_attributes();
        let weight = FontWeight::new(normalize_weight(attributes.weight.value()));
        let style = match attributes.style {
            SkrifaStyle::Normal => FontStyle::Normal,
            SkrifaStyle::Italic => FontStyle::Italic,
            SkrifaStyle::Oblique(_) => FontStyle::Oblique,
        };
        let stretch = FontStretch::new(normalize_stretch(attributes.width.ratio()));
        let family_key = normalize_family(&family).into_boxed_str();
        Ok(Self {
            index,
            units_per_em,
            has_color_tables,
            has_bitmap_tables,
            font,
            family,
            family_key,
            family_names,
            full_name,
            weight,
            style,
            stretch,
        })
    }

    /// The preferred family name, or an empty string when the font has no name table.
    ///
    /// The typographic family name (name ID 16) is used when present, otherwise the legacy
    /// family name (name ID 1). The result prefers an English entry, like CSS family matching.
    pub fn family_name(&self) -> &str {
        &self.family
    }

    /// All localized family names declared by the font.
    ///
    /// This is what family matching searches, so callers can refer to a font by its localized
    /// name (for example "思源黑体") as well as its English name.
    pub fn family_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.family_names.iter().map(|name| name.as_ref())
    }

    /// The full name (name ID 4), or an empty string when the font has none.
    pub fn full_name(&self) -> &str {
        &self.full_name
    }

    /// The face's weight.
    pub fn weight(&self) -> FontWeight {
        self.weight
    }

    /// The face's style.
    pub fn style(&self) -> FontStyle {
        self.style
    }

    /// The face's stretch.
    pub fn stretch(&self) -> FontStretch {
        self.stretch
    }

    /// The preferred family name, normalized for case-insensitive grouping.
    pub(crate) fn family_key(&self) -> &str {
        &self.family_key
    }

    /// The fontations font model, including the shaping caches harfrust attaches to it.
    pub(crate) fn font(&self) -> &Font {
        &self.font
    }

    /// Font design units per em.
    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// Whether the font's character map has an entry for `ch`.
    pub fn has_char(&self, ch: char) -> bool {
        self.font.charmap().map_unicode(ch).is_some()
    }

    pub(crate) fn glyph_for(&self, ch: char) -> Option<GlyphId> {
        self.font.charmap().map_unicode(ch)
    }

    /// Whether `glyph_id` is painted with a color representation (COLRv0 or a color bitmap).
    ///
    /// Monochrome and grayscale embedded bitmaps (EBDT) are coverage glyphs and are not
    /// considered color, so the text color still applies to them.
    pub(crate) fn has_color_glyph(&self, glyph_id: GlyphId) -> bool {
        if !self.has_color_tables {
            return false;
        }
        if self
            .font
            .color_glyphs()
            .get_with_version(glyph_id, ColorVersion::V0)
            .is_some()
        {
            return true;
        }
        let strikes = self.font.bitmap_strikes();
        if !matches!(
            strikes.source(),
            Some(BitmapSource::Cbdt | BitmapSource::Sbix)
        ) {
            return false;
        }
        strikes.iter().any(|strike| strike.get(glyph_id).is_some())
    }

    pub(crate) fn has_color_tables(&self) -> bool {
        self.has_color_tables
    }

    pub(crate) fn has_bitmap_tables(&self) -> bool {
        self.has_bitmap_tables
    }

    pub(crate) fn metrics(&self, size: f32) -> FontMetrics {
        let metrics = self.font.metrics();
        let Some(line) = metrics.h_line() else {
            return FontMetrics::default();
        };
        let scale = size / metrics.units_per_em.max(1) as f32;
        FontMetrics {
            ascent: line.ascender.to_f32() * scale,
            // The model stores the descender as a negative offset from the baseline, while
            // `FontMetrics` stores it as a positive distance.
            descent: -line.descender.to_f32() * scale,
            leading: line.line_gap.to_f32() * scale,
        }
    }
}

/// Reads the family and full names used for matching.
fn read_font_names(font: &Font) -> (Box<str>, Box<[Box<str>]>, Box<str>) {
    let typographic = collect_names(font, NameId::TYPOGRAPHIC_FAMILY_NAME);
    let (id, mut names) = if typographic.is_empty() {
        (
            NameId::FAMILY_NAME,
            collect_names(font, NameId::FAMILY_NAME),
        )
    } else {
        (NameId::TYPOGRAPHIC_FAMILY_NAME, typographic)
    };
    let family = preferred_name(font, id)
        .or_else(|| names.first().cloned())
        .unwrap_or_default();
    if names.is_empty() && !family.is_empty() {
        names.push(family.clone());
    }
    let full_name = preferred_name(font, NameId::FULL_NAME).unwrap_or_default();
    (family, names.into_boxed_slice(), full_name)
}

/// Collects and de-duplicates all names with the given id.
fn collect_names(font: &Font, id: NameId) -> Vec<Box<str>> {
    let mut names: Vec<Box<str>> = Vec::new();
    let Some(name) = font.names().get(id) else {
        return names;
    };
    for name in name.iter() {
        let value = name.name().to_string();
        let value = value.trim();
        if value.is_empty() || names.iter().any(|existing| existing.as_ref() == value) {
            continue;
        }
        names.push(value.into());
    }
    names
}

/// The best English (or first) name with the given id.
fn preferred_name(font: &Font, id: NameId) -> Option<Box<str>> {
    font.names()
        .get(id)?
        .english_or_first()
        .map(|name| name.name().to_string())
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .map(Into::into)
}

fn normalize_weight(weight: f32) -> u16 {
    if weight.is_finite() && weight > 0.0 {
        weight.round().clamp(1.0, 1000.0) as u16
    } else {
        FontWeight::NORMAL.value()
    }
}

fn normalize_stretch(ratio: f32) -> u16 {
    if ratio.is_finite() && ratio > 0.0 {
        (ratio * 100.0).round().clamp(1.0, 1000.0) as u16
    } else {
        FontStretch::NORMAL.value()
    }
}

/// Normalizes a family name for case-insensitive lookup.
fn normalize_family(name: &str) -> String {
    name.trim().to_lowercase()
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

/// An ordered list of faces, in load order.
///
/// The first face is the primary font. Faces are also indexed by their declared family names so
/// that a [`crate::TextStyle`] can prefer a specific family order regardless of load order.
#[derive(Clone, Debug, Default)]
pub struct FontStack {
    faces: Vec<FontFace>,
    by_family: HashMap<Box<str>, Vec<FontId>>,
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

    /// Whether any loaded face declares `family` (case-insensitive, including localized names).
    pub fn has_family(&self, family: &str) -> bool {
        let key = normalize_family(family);
        !key.is_empty() && self.by_family.contains_key(key.as_str())
    }

    pub fn get_all_families(&self) -> impl Iterator<Item = &str> {
        self.by_family.keys().map(|s| s.as_ref())
    }

    pub(crate) fn push(&mut self, face: FontFace) -> FontId {
        let id = FontId(self.faces.len() as u32);
        for name in face.family_names() {
            let key = normalize_family(name);
            if key.is_empty() {
                continue;
            }
            let ids = self.by_family.entry(key.into()).or_default();
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        self.faces.push(face);
        id
    }

    /// Resolves a style's preferred families and attributes into an ordered candidate list.
    pub(crate) fn resolve(
        &self,
        families: &FontFamilies,
        weight: FontWeight,
        style: FontStyle,
        stretch: FontStretch,
    ) -> FontSelection {
        let mut selection = FontSelection {
            order: Vec::with_capacity(self.faces.len()),
            preferred: 0,
        };
        let mut used = vec![false; self.faces.len()];

        if !families.is_empty() {
            for family in families {
                let key = normalize_family(family);
                if key.is_empty() {
                    continue;
                }
                let Some(ids) = self.by_family.get(key.as_str()) else {
                    continue;
                };
                let Some(best) = self.best_match(ids, weight, style, stretch) else {
                    continue;
                };
                if push_unique(&mut selection.order, &mut used, best) {
                    selection.preferred += 1;
                }
            }
        } else {
            // No explicit families: preserve load order, but pick the face that best matches the
            // requested attributes within each family.
            let mut seen: HashSet<&str> = HashSet::new();
            for (index, face) in self.faces.iter().enumerate() {
                let key = face.family_key();
                if key.is_empty() {
                    push_unique(&mut selection.order, &mut used, FontId(index as u32));
                    continue;
                }
                if !seen.insert(key) {
                    continue;
                }
                let Some(ids) = self.by_family.get(key) else {
                    continue;
                };
                if let Some(best) = self.best_match(ids, weight, style, stretch) {
                    push_unique(&mut selection.order, &mut used, best);
                }
            }
            // Only the first family is preferred when the style does not name families; the rest
            // are fallback faces, exactly like the old flat stack order.
            selection.preferred = usize::from(!selection.order.is_empty());
        }

        // Faces outside the preferred families stay available for glyph fallback.
        for index in 0..self.faces.len() {
            push_unique(&mut selection.order, &mut used, FontId(index as u32));
        }

        selection
    }

    /// The face in `ids` that best matches the requested attributes (CSS font matching).
    fn best_match(
        &self,
        ids: &[FontId],
        weight: FontWeight,
        style: FontStyle,
        stretch: FontStretch,
    ) -> Option<FontId> {
        best_candidate(
            ids.iter().map(|&id| {
                let face = &self.faces[id.0 as usize];
                (id, face.weight, face.style, face.stretch)
            }),
            weight,
            style,
            stretch,
        )
    }

    /// Picks a face for one grapheme cluster.
    ///
    /// The first preferred candidate wins whenever it covers the cluster. With the `fallback`
    /// feature (on by default) later candidates are tried when it does not. Emoji clusters prefer
    /// the first candidate that has a color representation for them.
    pub(crate) fn select(
        &self,
        selection: &FontSelection,
        cluster: &str,
        allow_fallback: bool,
    ) -> Option<FontId> {
        let candidates = selection.candidates(allow_fallback);
        let primary_id = *candidates.first()?;
        if candidates.len() == 1 {
            return Some(primary_id);
        }

        let chars = cluster_chars(cluster);
        if chars.is_empty() {
            return Some(primary_id);
        }

        let prefer_color = cluster_prefers_color(cluster);
        let mut first_cover = None;
        for &id in candidates {
            let Some(face) = self.get(id) else {
                continue;
            };
            let covers_all = chars.iter().all(|&ch| face.has_char(ch));
            if !covers_all {
                continue;
            }
            if !prefer_color {
                // Candidates are in fallback order, so the first covering face wins.
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

/// The resolved candidate order for one layout.
#[derive(Clone, Debug, Default)]
pub(crate) struct FontSelection {
    order: Vec<FontId>,
    /// The number of leading candidates that come from explicitly requested families (or the
    /// first family when none are requested). These are tried even without the `fallback`
    /// feature; the rest of the order is implicit fallback.
    preferred: usize,
}

impl FontSelection {
    /// The face whose metrics define the layout.
    pub(crate) fn primary(&self) -> Option<FontId> {
        self.order.first().copied()
    }

    /// The faces that may be used for a cluster.
    pub(crate) fn candidates(&self, allow_fallback: bool) -> &[FontId] {
        let len = if allow_fallback {
            self.order.len()
        } else {
            self.preferred.max(1).min(self.order.len())
        };
        &self.order[..len]
    }
}

fn push_unique(order: &mut Vec<FontId>, used: &mut [bool], id: FontId) -> bool {
    let index = id.0 as usize;
    if used[index] {
        return false;
    }
    used[index] = true;
    order.push(id);
    true
}

/// Picks the first candidate under the CSS ordering of style, stretch and weight.
fn best_candidate<I>(
    candidates: I,
    weight: FontWeight,
    style: FontStyle,
    stretch: FontStretch,
) -> Option<FontId>
where
    I: IntoIterator<Item = (FontId, FontWeight, FontStyle, FontStretch)>,
{
    candidates
        .into_iter()
        .min_by_key(|&(_, face_weight, face_style, face_stretch)| {
            (
                style_rank(face_style, style),
                stretch_rank(face_stretch, stretch),
                weight_rank(face_weight, weight),
            )
        })
        .map(|(id, ..)| id)
}

/// CSS style matching: exact first, then the nearest slope.
fn style_rank(available: FontStyle, requested: FontStyle) -> u8 {
    if available == requested {
        return 0;
    }
    match (requested, available) {
        // Normal prefers oblique before italic.
        (FontStyle::Normal, FontStyle::Oblique) => 1,
        (FontStyle::Normal, FontStyle::Italic) => 2,
        // Italic prefers oblique before normal.
        (FontStyle::Italic, FontStyle::Oblique) => 1,
        (FontStyle::Italic, FontStyle::Normal) => 2,
        // Oblique prefers italic before normal.
        (FontStyle::Oblique, FontStyle::Italic) => 1,
        (FontStyle::Oblique, FontStyle::Normal) => 2,
        _ => 0,
    }
}

/// CSS stretch matching: exact, then narrower, then wider (or wider first above 100%).
fn stretch_rank(available: FontStretch, requested: FontStretch) -> (u8, u16) {
    let available = available.value() as i32;
    let requested = requested.value() as i32;
    if available == requested {
        (0, 0)
    } else if requested <= 100 {
        if available < requested {
            (1, (requested - available) as u16)
        } else {
            (2, (available - requested) as u16)
        }
    } else if available > requested {
        (1, (available - requested) as u16)
    } else {
        (2, (requested - available) as u16)
    }
}

/// CSS weight matching.
fn weight_rank(available: FontWeight, requested: FontWeight) -> (u8, u16) {
    let available = available.value() as i32;
    let requested = requested.value() as i32;
    if requested < 400 {
        if available <= requested {
            (0, (requested - available) as u16)
        } else {
            (1, (available - requested) as u16)
        }
    } else if requested <= 500 {
        if available >= requested && available <= 500 {
            (0, (available - requested) as u16)
        } else if available < requested {
            (1, (requested - available) as u16)
        } else {
            (2, (available - 500) as u16)
        }
    } else if available >= requested {
        (0, (available - requested) as u16)
    } else {
        (1, (requested - available) as u16)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collections_share_faces_and_report_bad_indices() {
        let data: Arc<[u8]> = font_test_data::ttc::TTC.to_vec().into();
        assert_eq!(FontFace::from_shared_collection(data.clone()).len(), 2);

        assert!(matches!(
            FontFace::from_shared(data, 2),
            Err(FontError::InvalidIndex(2))
        ));
        assert!(matches!(
            FontFace::from_shared(b"not a font".to_vec().into(), 0),
            Err(FontError::InvalidFont)
        ));
    }

    /// Removes CBDT/CBLC from a font directory, leaving its EBDT/EBLC strikes.
    ///
    /// Table data stays at its original offsets; only the directory shrinks.
    fn without_color_bitmap_tables(data: &[u8]) -> Vec<u8> {
        let count = u16::from_be_bytes([data[4], data[5]]) as usize;
        let records: Vec<&[u8]> = (0..count)
            .map(|index| {
                let start = 12 + index * 16;
                &data[start..start + 16]
            })
            .filter(|record| {
                let tag = &record[..4];
                tag != &b"CBDT"[..] && tag != &b"CBLC"[..]
            })
            .collect();
        let mut out = data.to_vec();
        let count = records.len();
        out[4..6].copy_from_slice(&(count as u16).to_be_bytes());
        let max_power = if count == 0 {
            0
        } else {
            1usize << count.ilog2()
        };
        out[6..8].copy_from_slice(&((max_power * 16) as u16).to_be_bytes());
        out[8..10].copy_from_slice(&(max_power.trailing_zeros() as u16).to_be_bytes());
        out[10..12]
            .copy_from_slice(&((count * 16).saturating_sub(max_power * 16) as u16).to_be_bytes());
        for (index, record) in records.iter().enumerate() {
            let start = 12 + index * 16;
            out[start..start + 16].copy_from_slice(record);
        }
        out
    }

    #[test]
    fn monochrome_embedded_bitmaps_are_coverage() {
        let data = without_color_bitmap_tables(font_test_data::EMBEDDED_BITMAPS);
        let face = FontFace::new(data, 0).unwrap();
        assert!(face.has_bitmap_tables());
        assert!(!face.has_color_tables(), "EBDT is not a color table");

        let font = face.font();
        assert_eq!(font.bitmap_strikes().source(), Some(BitmapSource::Ebdt));
        let glyph_id = (0..font.num_glyphs())
            .map(GlyphId::new)
            .find(|glyph_id| {
                font.bitmap_strikes()
                    .iter()
                    .any(|strike| strike.get(*glyph_id).is_some())
            })
            .expect("the test font has an EBDT glyph");

        assert!(!face.has_color_glyph(glyph_id));
        let raster =
            crate::rasterize(&face, glyph_id.to_u32(), 16.0).expect("the EBDT glyph rasterizes");
        assert!(
            !raster.color,
            "coverage glyphs are tinted with the text color"
        );
    }

    fn candidate(
        id: u32,
        weight: u16,
        style: FontStyle,
        stretch: u16,
    ) -> (FontId, FontWeight, FontStyle, FontStretch) {
        (
            FontId::from_index(id),
            FontWeight::new(weight),
            style,
            FontStretch::new(stretch),
        )
    }

    fn pick(
        candidates: impl IntoIterator<Item = (FontId, FontWeight, FontStyle, FontStretch)>,
        weight: u16,
        style: FontStyle,
        stretch: u16,
    ) -> Option<FontId> {
        best_candidate(
            candidates,
            FontWeight::new(weight),
            style,
            FontStretch::new(stretch),
        )
    }

    #[test]
    fn weight_matching_follows_css_preferences() {
        // 400 takes the nearest lighter weight when 500 is not available.
        assert_eq!(
            pick(
                [
                    candidate(0, 300, FontStyle::Normal, 100),
                    candidate(1, 700, FontStyle::Normal, 100),
                ],
                400,
                FontStyle::Normal,
                100,
            ),
            Some(FontId::from_index(0))
        );
        // Between 400 and 500, a heavier face wins before a lighter one.
        assert_eq!(
            pick(
                [
                    candidate(0, 300, FontStyle::Normal, 100),
                    candidate(1, 500, FontStyle::Normal, 100),
                ],
                400,
                FontStyle::Normal,
                100,
            ),
            Some(FontId::from_index(1))
        );
        // Above 500, a heavier face wins.
        assert_eq!(
            pick(
                [
                    candidate(0, 500, FontStyle::Normal, 100),
                    candidate(1, 700, FontStyle::Normal, 100),
                ],
                600,
                FontStyle::Normal,
                100,
            ),
            Some(FontId::from_index(1))
        );
    }

    #[test]
    fn style_matching_prefers_the_nearest_slope() {
        assert_eq!(
            pick(
                [
                    candidate(0, 400, FontStyle::Normal, 100),
                    candidate(1, 400, FontStyle::Oblique, 100),
                ],
                400,
                FontStyle::Italic,
                100,
            ),
            Some(FontId::from_index(1))
        );
        assert_eq!(
            pick(
                [
                    candidate(0, 400, FontStyle::Normal, 100),
                    candidate(1, 400, FontStyle::Italic, 100),
                ],
                400,
                FontStyle::Oblique,
                100,
            ),
            Some(FontId::from_index(1))
        );
        assert_eq!(
            pick(
                [
                    candidate(0, 400, FontStyle::Italic, 100),
                    candidate(1, 400, FontStyle::Oblique, 100),
                ],
                400,
                FontStyle::Normal,
                100,
            ),
            Some(FontId::from_index(1))
        );
    }

    #[test]
    fn stretch_matching_prefers_narrower_before_wider() {
        assert_eq!(
            pick(
                [
                    candidate(0, 400, FontStyle::Normal, 125),
                    candidate(1, 400, FontStyle::Normal, 75),
                ],
                400,
                FontStyle::Normal,
                100,
            ),
            Some(FontId::from_index(1))
        );
        // Above normal, wider faces are preferred first.
        assert_eq!(
            pick(
                [
                    candidate(0, 400, FontStyle::Normal, 125),
                    candidate(1, 400, FontStyle::Normal, 200),
                ],
                400,
                FontStyle::Normal,
                150,
            ),
            Some(FontId::from_index(1))
        );
    }

    #[test]
    fn ties_keep_stack_order() {
        assert_eq!(
            pick(
                [
                    candidate(3, 400, FontStyle::Normal, 100),
                    candidate(1, 400, FontStyle::Normal, 100),
                ],
                400,
                FontStyle::Normal,
                100,
            ),
            Some(FontId::from_index(3))
        );
    }

    #[test]
    fn family_names_are_case_insensitive() {
        assert_eq!(normalize_family("  Noto Sans  "), "noto sans");
    }
}
