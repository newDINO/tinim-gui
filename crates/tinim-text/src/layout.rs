//! Text styles, line breaking and the result of laying out a string.

use std::ops::Range;

use glam::{Vec2, vec2};
use unicode_bidi::{BidiInfo, Level, ParagraphInfo};
use unicode_linebreak::{BreakOpportunity, linebreaks};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    font::{FontId, FontMetrics, FontStack},
    shape,
};

/// The base direction of a paragraph.
///
/// [`TextDirection::Auto`] follows the first strong character, like HTML's `dir=auto`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextDirection {
    /// Pick the direction from the text itself.
    #[default]
    Auto,
    LeftToRight,
    RightToLeft,
}

/// How text should be laid out.
///
/// Paint-only inputs such as color are deliberately absent: layout is independent of how the
/// resulting glyphs are drawn. Pass the color to the paint layer when building a mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    /// Font size in logical points.
    pub font_size: f32,
    /// Distance between baselines in logical points. Defaults to the font's metrics.
    pub line_height: Option<f32>,
    /// Extra horizontal spacing after every glyph, in logical points.
    pub letter_spacing: f32,
    /// Base direction of every paragraph.
    pub direction: TextDirection,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            line_height: None,
            letter_spacing: 0.0,
            direction: TextDirection::Auto,
        }
    }
}

/// The normalized layout-affecting part of [`TextStyle`].
///
/// This is used for comparison and hashing. Paint-only inputs are not part of [`TextStyle`] at
/// all, so two layouts can always be reused regardless of the color they are drawn with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LayoutStyle {
    font_size: u32,
    line_height: Option<u32>,
    letter_spacing: u32,
    direction: TextDirection,
}

impl LayoutStyle {
    /// Builds a canonical layout style from `style`.
    ///
    /// Invalid values are normalized exactly like layout does, so the result can be used as a
    /// cache key without duplicating equivalent layouts.
    pub fn new(style: &TextStyle) -> Self {
        let font_size = if style.font_size.is_finite() && style.font_size > 0.0 {
            style.font_size
        } else {
            12.0
        };
        let line_height = style
            .line_height
            .filter(|height| height.is_finite() && *height >= 0.0);
        let letter_spacing = if style.letter_spacing.is_finite() {
            style.letter_spacing
        } else {
            0.0
        };
        Self {
            font_size: canonical_bits(font_size),
            line_height: line_height.map(canonical_bits),
            letter_spacing: canonical_bits(letter_spacing),
            direction: style.direction,
        }
    }

    /// Font size in logical points.
    pub fn font_size(&self) -> f32 {
        f32::from_bits(self.font_size)
    }

    /// Distance between baselines in logical points, if explicitly set.
    pub fn line_height(&self) -> Option<f32> {
        self.line_height.map(f32::from_bits)
    }

    /// Extra horizontal spacing after every glyph, in logical points.
    pub fn letter_spacing(&self) -> f32 {
        f32::from_bits(self.letter_spacing)
    }

    /// Base direction of every paragraph.
    pub fn direction(&self) -> TextDirection {
        self.direction
    }
}

/// Canonical inputs that fully determine a line layout: style plus wrap width.
///
/// Use this as a cache key component so the cache and the layout code agree on normalization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LayoutKey {
    style: LayoutStyle,
    /// Normalized wrap width in bits. `None` means unlimited.
    max_width: Option<u32>,
}

impl LayoutKey {
    /// Builds the canonical layout inputs from a text style and wrap width.
    pub fn new(style: &TextStyle, max_width: Option<f32>) -> Self {
        Self {
            style: LayoutStyle::new(style),
            max_width: max_width.and_then(canonical_width),
        }
    }

    /// The normalized layout-affecting style.
    pub fn style(&self) -> &LayoutStyle {
        &self.style
    }

    /// The normalized wrap width. `None` means unlimited width.
    pub fn max_width(&self) -> Option<f32> {
        self.max_width.map(f32::from_bits)
    }
}

/// Normalizes `-0.0` to `0.0` so numerically equal values have equal bits.
fn canonical_bits(value: f32) -> u32 {
    if value == 0.0 {
        0.0f32.to_bits()
    } else {
        value.to_bits()
    }
}

/// Mirrors the normalization in [`wrap_lines`]: negative and NaN widths act like zero, while
/// infinity acts like no limit.
fn canonical_width(width: f32) -> Option<u32> {
    if width == f32::INFINITY {
        None
    } else {
        Some(canonical_bits(width.max(0.0)))
    }
}

/// One shaped glyph, positioned relative to the start of its line.
#[derive(Clone, Debug)]
pub struct PositionedGlyph {
    pub(crate) font: FontId,
    pub(crate) glyph_id: u32,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) advance: f32,
    pub(crate) color: bool,
}

impl PositionedGlyph {
    /// The face that should be used to rasterize this glyph.
    pub fn font(&self) -> FontId {
        self.font
    }

    /// Glyph identifier inside [`PositionedGlyph::font`].
    pub fn glyph_id(&self) -> u32 {
        self.glyph_id
    }

    /// Pen position from the left edge of the line, in logical points.
    pub fn x(&self) -> f32 {
        self.x
    }

    /// Vertical offset from the baseline (positive is up), in logical points.
    pub fn y_offset(&self) -> f32 {
        self.y
    }

    /// Horizontal advance including letter spacing, in logical points.
    pub fn advance(&self) -> f32 {
        self.advance
    }

    /// Whether this glyph has a color representation.
    pub fn is_color(&self) -> bool {
        self.color
    }
}

/// One line of a [`TextLayout`].
#[derive(Clone, Debug, Default)]
pub struct TextLine {
    glyphs: Vec<PositionedGlyph>,
    width: f32,
    baseline: f32,
}

impl TextLine {
    pub fn glyphs(&self) -> &[PositionedGlyph] {
        &self.glyphs
    }

    /// Width of the line in logical points.
    pub fn width(&self) -> f32 {
        self.width
    }

    /// Distance from the top of the layout to this line's baseline, in logical points.
    pub fn baseline(&self) -> f32 {
        self.baseline
    }
}

/// A laid out string, independent of any glyph atlas or paint backend.
///
/// Layout only shapes text: it does not rasterize anything. Use [`crate::rasterize`] to turn
/// individual glyphs into bitmaps, or a paint integration such as `tinim-paint-text` to build a
/// mesh.
#[derive(Clone, Debug)]
pub struct TextLayout {
    key: LayoutKey,
    lines: Vec<TextLine>,
    size: Vec2,
}

impl TextLayout {
    /// The normalized inputs used for layout.
    pub fn layout_key(&self) -> LayoutKey {
        self.key
    }

    /// The layout-affecting style. Paint-only fields such as color are not part of a layout.
    pub fn style(&self) -> &LayoutStyle {
        self.key.style()
    }

    /// The wrap width used for layout. `None` means unlimited width.
    pub fn max_width(&self) -> Option<f32> {
        self.key.max_width()
    }

    pub fn lines(&self) -> &[TextLine] {
        &self.lines
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Size of the laid out block in logical points.
    pub fn size(&self) -> Vec2 {
        self.size
    }

    pub fn width(&self) -> f32 {
        self.size.x
    }

    pub fn height(&self) -> f32 {
        self.size.y
    }
}

pub(crate) fn layout(
    fonts: &FontStack,
    text: &str,
    style: &TextStyle,
    max_width: Option<f32>,
    allow_fallback: bool,
) -> TextLayout {
    let key = LayoutKey::new(style, max_width);
    let style = *key.style();
    let max_width = key.max_width();
    let mut result = TextLayout {
        key,
        lines: Vec::new(),
        size: Vec2::ZERO,
    };
    if text.is_empty() {
        return result;
    }

    let base_level = match style.direction() {
        TextDirection::Auto => None,
        TextDirection::LeftToRight => Some(Level::ltr()),
        TextDirection::RightToLeft => Some(Level::rtl()),
    };
    let bidi = BidiInfo::new(text, base_level);
    let ranges = wrap_lines(fonts, text, &style, max_width, &bidi, allow_fallback);

    let metrics = fonts
        .primary()
        .map(|(_, face)| face.metrics(style.font_size()))
        .unwrap_or_else(|| FontMetrics::fallback(style.font_size()));
    let line_height = style
        .line_height()
        .unwrap_or_else(|| metrics.line_height())
        .max(0.0);
    let leading = line_height - (metrics.ascent + metrics.descent);
    let first_baseline = leading * 0.5 + metrics.ascent;

    let mut width: f32 = 0.0;
    for (index, range) in ranges.into_iter().enumerate() {
        let paragraph = paragraph_for(&bidi, range.start);
        let (glyphs, line_width) =
            shape::shape_line(fonts, text, &bidi, paragraph, range, &style, allow_fallback);
        width = width.max(line_width);
        result.lines.push(TextLine {
            glyphs,
            width: line_width,
            baseline: first_baseline + index as f32 * line_height,
        });
    }

    result.size = vec2(width, result.lines.len() as f32 * line_height);
    result
}

fn wrap_lines(
    fonts: &FontStack,
    text: &str,
    style: &LayoutStyle,
    max_width: Option<f32>,
    bidi: &BidiInfo<'_>,
    allow_fallback: bool,
) -> Vec<Range<usize>> {
    let max_width = max_width.unwrap_or(f32::INFINITY);
    let breaks: Vec<(usize, BreakOpportunity)> = linebreaks(text).collect();
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut break_index = 0usize;

    while start < text.len() {
        // The first break opportunity that advances past the current line start.
        let mut first_index = break_index;
        while first_index < breaks.len() && breaks[first_index].0 <= start {
            first_index += 1;
        }

        // Only consider breaks up to (and including) the next mandatory break. A paragraph must
        // not be joined with the following one even when the width would allow it.
        let mut last_index = first_index;
        while last_index < breaks.len() && breaks[last_index].1 != BreakOpportunity::Mandatory {
            last_index += 1;
        }
        if last_index >= breaks.len() {
            last_index = breaks.len().saturating_sub(1);
        }

        let candidate = |index: usize| {
            let position = breaks[index].0;
            let content_end = trim_end(text, start, position);
            (position, content_end)
        };

        let best = if first_index >= breaks.len() {
            None
        } else {
            // Most lines fit completely, so check the last candidate before searching for the
            // first one that overflows. This turns the common case into a single measurement.
            let (_, last_end) = candidate(last_index);
            if max_width == f32::INFINITY
                || measure(fonts, text, bidi, start..last_end, style, allow_fallback) <= max_width
            {
                Some(last_index)
            } else {
                // Widths are non-decreasing as more text is added (the same assumption the
                // linear scan made), so binary search for the last candidate that fits.
                let mut lo = first_index;
                let mut hi = last_index;
                let mut best = None;
                while lo < hi {
                    let mid = lo + (hi - lo) / 2;
                    let (_, content_end) = candidate(mid);
                    if measure(fonts, text, bidi, start..content_end, style, allow_fallback)
                        <= max_width
                    {
                        best = Some(mid);
                        lo = mid + 1;
                    } else {
                        hi = mid;
                    }
                }
                best
            }
        };

        let (position, content_end) = match best {
            Some(index) => {
                let (position, content_end) = candidate(index);
                break_index = index + 1;
                (position, content_end)
            }
            None => {
                let (fallback_position, fallback_end) = if first_index < breaks.len() {
                    candidate(first_index)
                } else {
                    (text.len(), trim_end(text, start, text.len()))
                };
                match emergency_break(
                    fonts,
                    text,
                    bidi,
                    start..fallback_end,
                    style,
                    max_width,
                    allow_fallback,
                ) {
                    Some((position, content_end)) => (position, content_end),
                    None => (fallback_position, fallback_end),
                }
            }
        };
        lines.push(start..content_end);
        start = position;
    }

    lines
}

/// Breaks inside an over-long word at a grapheme boundary. Returns the position where the next
/// line starts and the trimmed end of this line.
fn emergency_break(
    fonts: &FontStack,
    text: &str,
    bidi: &BidiInfo<'_>,
    range: Range<usize>,
    style: &LayoutStyle,
    max_width: f32,
    allow_fallback: bool,
) -> Option<(usize, usize)> {
    if range.is_empty() {
        return None;
    }

    let mut first_end = None;
    let mut last_fit = None;
    for (offset, grapheme) in text[range.clone()].grapheme_indices(true) {
        let grapheme_end = range.start + offset + grapheme.len();
        first_end.get_or_insert(grapheme_end);
        let content_end = trim_end(text, range.start, grapheme_end);
        let width = measure(
            fonts,
            text,
            bidi,
            range.start..content_end,
            style,
            allow_fallback,
        );
        if width <= max_width {
            last_fit = Some((grapheme_end, content_end));
        } else {
            break;
        }
    }

    last_fit.or_else(|| first_end.map(|end| (end, trim_end(text, range.start, end))))
}

fn measure(
    fonts: &FontStack,
    text: &str,
    bidi: &BidiInfo<'_>,
    range: Range<usize>,
    style: &LayoutStyle,
    allow_fallback: bool,
) -> f32 {
    if range.is_empty() {
        return 0.0;
    }
    let paragraph = paragraph_for(bidi, range.start);
    shape::shape_line(fonts, text, bidi, paragraph, range, style, allow_fallback).1
}

fn paragraph_for<'a>(bidi: &'a BidiInfo<'_>, position: usize) -> &'a ParagraphInfo {
    bidi.paragraphs
        .iter()
        .find(|paragraph| paragraph.range.start <= position && position < paragraph.range.end)
        .or_else(|| bidi.paragraphs.last())
        .expect("BidiInfo of a non-empty string has at least one paragraph")
}

/// Trims spaces and line terminators from the end of a line, without touching NBSP.
fn trim_end(text: &str, start: usize, end: usize) -> usize {
    let mut end = end.min(text.len());
    while end > start {
        let ch = text[..end]
            .chars()
            .next_back()
            .expect("end > start implies a character");
        if matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{000B}' | '\u{000C}') {
            end -= ch.len_utf8();
        } else {
            break;
        }
    }
    end
}
