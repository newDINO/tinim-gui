//! Text styles, line breaking and the result of laying out a string.

use std::ops::Range;

use tinim_paint::{Color32, Vec2, vec2};
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

/// How text should be rendered.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    /// Font size in logical points.
    pub font_size: f32,
    /// Text color. Color glyphs (emoji) only use its alpha.
    pub color: Color32,
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
            color: Color32::BLACK,
            line_height: None,
            letter_spacing: 0.0,
            direction: TextDirection::Auto,
        }
    }
}

impl TextStyle {
    fn sanitized(&self) -> Self {
        let mut style = self.clone();
        style.font_size = if self.font_size.is_finite() && self.font_size > 0.0 {
            self.font_size
        } else {
            12.0
        };
        style.line_height = self
            .line_height
            .filter(|height| height.is_finite() && *height >= 0.0);
        style.letter_spacing = if self.letter_spacing.is_finite() {
            self.letter_spacing
        } else {
            0.0
        };
        style
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

/// A laid out string, independent of the glyph atlas.
///
/// Layout only shapes text: it does not rasterize anything. Call
/// [`crate::TextSystem::prepare`] to turn it into a mesh.
#[derive(Clone, Debug)]
pub struct TextLayout {
    text: String,
    style: TextStyle,
    lines: Vec<TextLine>,
    size: Vec2,
}

impl TextLayout {
    /// The original text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The style used for layout.
    pub fn style(&self) -> &TextStyle {
        &self.style
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
    let style = style.sanitized();
    let mut result = TextLayout {
        text: text.to_owned(),
        style: style.clone(),
        lines: Vec::new(),
        size: Vec2::ZERO,
    };
    if text.is_empty() {
        return result;
    }

    let base_level = match style.direction {
        TextDirection::Auto => None,
        TextDirection::LeftToRight => Some(Level::ltr()),
        TextDirection::RightToLeft => Some(Level::rtl()),
    };
    let bidi = BidiInfo::new(text, base_level);
    let ranges = wrap_lines(fonts, text, &style, max_width, &bidi, allow_fallback);

    let metrics = fonts
        .primary()
        .map(|(_, face)| face.metrics(style.font_size))
        .unwrap_or_else(|| FontMetrics::fallback(style.font_size));
    let line_height = style
        .line_height
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
    style: &TextStyle,
    max_width: Option<f32>,
    bidi: &BidiInfo<'_>,
    allow_fallback: bool,
) -> Vec<Range<usize>> {
    let max_width = max_width
        .map(|width| width.max(0.0))
        .unwrap_or(f32::INFINITY);
    let breaks: Vec<(usize, BreakOpportunity)> = linebreaks(text).collect();
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut break_index = 0usize;

    while start < text.len() {
        let mut best = None;
        let mut first = None;
        let mut index = break_index;

        while index < breaks.len() {
            let (position, opportunity) = breaks[index];
            if position <= start {
                index += 1;
                continue;
            }

            let content_end = trim_end(text, start, position);
            let mandatory = opportunity == BreakOpportunity::Mandatory;
            if first.is_none() {
                first = Some((position, content_end, mandatory));
            }
            let width = measure(fonts, text, bidi, start..content_end, style, allow_fallback);
            if width <= max_width {
                best = Some((position, content_end, mandatory));
                index += 1;
                if mandatory {
                    break;
                }
            } else {
                break;
            }
        }

        let (position, content_end, _mandatory) = match best {
            Some(best) => best,
            None => {
                let (fallback_position, fallback_end, fallback_mandatory) =
                    first.unwrap_or_else(|| (text.len(), trim_end(text, start, text.len()), true));
                match emergency_break(
                    fonts,
                    text,
                    bidi,
                    start..fallback_end,
                    style,
                    max_width,
                    allow_fallback,
                ) {
                    Some((position, content_end)) => (position, content_end, false),
                    None => (fallback_position, fallback_end, fallback_mandatory),
                }
            }
        };
        lines.push(start..content_end);
        start = position;
        break_index = index;
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
    style: &TextStyle,
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
    style: &TextStyle,
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
