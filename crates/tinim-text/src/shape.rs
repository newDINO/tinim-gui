//! Bidi resolution, itemization and OpenType shaping.
//!
//! Shaping happens once per paragraph, not once per line. The resulting runs keep their absolute
//! cluster positions, so line breaking can measure candidate prefixes from advances and the final
//! lines are assembled by slicing the already shaped glyphs.

use std::ops::Range;

use harfrust::{Direction, UnicodeBuffer};
use skrifa::GlyphId;
use unicode_bidi::{BidiInfo, ParagraphInfo};
use unicode_script::Script as UnicodeScript;
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    font::{FontId, FontStack},
    layout::{LayoutStyle, PositionedGlyph},
};

/// All glyphs of a text, shaped paragraph by paragraph.
pub(crate) struct ShapedText {
    items: Vec<ShapedItem>,
    text_len: usize,
    /// Cumulative advances at every byte offset, indexed by cluster start. Empty when the
    /// caller does not need to measure ranges (unbounded width).
    advance_prefix: Vec<f32>,
    /// Cumulative glyph counts at every byte offset, indexed by cluster start.
    count_prefix: Vec<u32>,
    letter_spacing: f32,
}

struct ShapedItem {
    range: Range<usize>,
    glyphs: Vec<PositionedGlyph>,
}

impl ShapedText {
    /// Shapes every paragraph of `text` once.
    ///
    /// `measure_widths` builds the prefix tables used by [`ShapedText::width`]. Skip it when the
    /// text cannot wrap; line widths are still available from [`ShapedText::line`].
    pub(crate) fn shape(
        fonts: &FontStack,
        text: &str,
        bidi: &BidiInfo<'_>,
        style: &LayoutStyle,
        allow_fallback: bool,
        measure_widths: bool,
    ) -> Self {
        let mut items = Vec::new();
        // Pure LTR text is by far the common case: its visual runs are just the paragraphs, so
        // skip unicode-bidi's per-byte level reordering entirely.
        let has_rtl = bidi.has_rtl();
        for paragraph in &bidi.paragraphs {
            let end = paragraph_content_end(text, paragraph.range.end);
            if paragraph.range.start >= end {
                continue;
            }
            let paragraph_range = paragraph.range.start..end;
            if !has_rtl {
                for item in itemize(fonts, text, paragraph_range, allow_fallback) {
                    if let Some(shaped) =
                        shape_item(fonts, text, item, Direction::LeftToRight, style)
                    {
                        items.push(shaped);
                    }
                }
                continue;
            }
            let (levels, runs) = bidi.visual_runs(paragraph, paragraph_range);
            for run in runs {
                if run.is_empty() {
                    continue;
                }
                let direction = if levels[run.start].is_rtl() {
                    Direction::RightToLeft
                } else {
                    Direction::LeftToRight
                };
                for item in itemize(fonts, text, run, allow_fallback) {
                    if let Some(shaped) = shape_item(fonts, text, item, direction, style) {
                        items.push(shaped);
                    }
                }
            }
        }
        // Runs are visited in visual order; line assembly looks items up by logical position.
        items.sort_by_key(|item| item.range.start);

        let letter_spacing = style.letter_spacing();
        let mut advance_prefix = Vec::new();
        let mut count_prefix = Vec::new();
        if measure_widths {
            advance_prefix = vec![0.0f32; text.len() + 1];
            count_prefix = vec![0u32; text.len() + 1];
            for item in &items {
                for glyph in &item.glyphs {
                    let cluster = glyph.cluster.start;
                    advance_prefix[cluster] += glyph.advance + letter_spacing;
                    count_prefix[cluster] += 1;
                }
            }
            // Turn the per-cluster buckets into exclusive prefix sums.
            let mut advances = 0.0f32;
            let mut count = 0u32;
            for index in 0..=text.len() {
                let advance = advance_prefix[index];
                let glyphs = count_prefix[index];
                advance_prefix[index] = advances;
                count_prefix[index] = count;
                advances += advance;
                count += glyphs;
            }
        }

        Self {
            items,
            text_len: text.len(),
            advance_prefix,
            count_prefix,
            letter_spacing,
        }
    }

    /// Width of a byte range, using the advances collected while shaping.
    pub(crate) fn width(&self, range: Range<usize>) -> f32 {
        if range.is_empty() {
            return 0.0;
        }
        debug_assert!(
            !self.advance_prefix.is_empty(),
            "width requires measure_widths"
        );
        let start = range.start.min(self.text_len);
        let end = range.end.min(self.text_len);
        if start >= end {
            return 0.0;
        }
        let mut width = self.advance_prefix[end] - self.advance_prefix[start];
        let count = self.count_prefix[end] - self.count_prefix[start];
        if count > 0 {
            // Letter spacing goes between glyphs, not after the last one.
            width -= self.letter_spacing;
        }
        width
    }

    /// Assembles one line in visual order from the glyphs shaped for its paragraph.
    pub(crate) fn line(
        &mut self,
        bidi: &BidiInfo<'_>,
        paragraph: &ParagraphInfo,
        line: Range<usize>,
        style: &LayoutStyle,
    ) -> (Vec<PositionedGlyph>, f32) {
        if line.is_empty() || line.end > self.text_len {
            return (Vec::new(), 0.0);
        }

        let has_rtl = bidi.has_rtl();
        if let Some(result) =
            self.take_single_item_line(bidi, paragraph, line.clone(), style, has_rtl)
        {
            return result;
        }

        let mut glyphs = Vec::new();
        let mut pen_x = 0.0;
        if !has_rtl {
            self.emit_range(&line, false, &mut pen_x, &mut glyphs);
        } else {
            let (levels, runs) = bidi.visual_runs(paragraph, line.clone());
            for run in runs {
                if run.is_empty() {
                    continue;
                }
                self.emit_range(&run, levels[run.start].is_rtl(), &mut pen_x, &mut glyphs);
            }
        }

        if let Some(last) = glyphs.last_mut() {
            last.advance -= self.letter_spacing;
            pen_x -= self.letter_spacing;
        }

        (glyphs, pen_x)
    }

    fn emit_range(
        &self,
        run: &Range<usize>,
        rtl: bool,
        pen_x: &mut f32,
        out: &mut Vec<PositionedGlyph>,
    ) {
        let first = self
            .items
            .partition_point(|item| item.range.end <= run.start);
        let mut end = first;
        while end < self.items.len() && self.items[end].range.start < run.end {
            end += 1;
        }
        let items = &self.items[first..end];

        // RTL runs are shaped in visual order internally, but their items are still in logical
        // order and therefore have to be appended in reverse.
        if rtl {
            for item in items.iter().rev() {
                emit(item, run, self.letter_spacing, pen_x, out);
            }
        } else {
            for item in items {
                emit(item, run, self.letter_spacing, pen_x, out);
            }
        }
    }

    /// Most labels and unwrapped lines are a single LTR run with a single item. Move its glyphs
    /// out instead of allocating and copying them.
    fn take_single_item_line(
        &mut self,
        bidi: &BidiInfo<'_>,
        paragraph: &ParagraphInfo,
        line: Range<usize>,
        style: &LayoutStyle,
        has_rtl: bool,
    ) -> Option<(Vec<PositionedGlyph>, f32)> {
        let run = if has_rtl {
            let (levels, runs) = bidi.visual_runs(paragraph, line.clone());
            let mut runs = runs.into_iter().filter(|run| !run.is_empty());
            let run = runs.next()?;
            if runs.next().is_some() || levels[run.start].is_rtl() {
                return None;
            }
            run
        } else {
            line
        };

        let index = self
            .items
            .partition_point(|item| item.range.end <= run.start);
        let (item_start, item_end) = {
            let item = self.items.get(index)?;
            (item.range.start, item.range.end)
        };
        // Only take items that belong entirely to this line; items spanning a break are sliced.
        if item_start < run.start || item_end > run.end {
            return None;
        }
        if self
            .items
            .get(index + 1)
            .is_some_and(|next| next.range.start < run.end)
        {
            return None;
        }

        let spacing = style.letter_spacing();
        let item = &mut self.items[index];
        let mut glyphs = std::mem::take(&mut item.glyphs);
        let mut pen_x = 0.0;
        for glyph in &mut glyphs {
            glyph.x += pen_x;
            glyph.advance += spacing;
            pen_x += glyph.advance;
        }
        if let Some(last) = glyphs.last_mut() {
            last.advance -= spacing;
            pen_x -= spacing;
        }
        Some((glyphs, pen_x))
    }
}

/// Appends the glyphs of `item` that fall inside `run`, rebasing them onto the line.
fn emit(
    item: &ShapedItem,
    run: &Range<usize>,
    spacing: f32,
    pen_x: &mut f32,
    out: &mut Vec<PositionedGlyph>,
) {
    for glyph in &item.glyphs {
        let cluster = glyph.cluster.start;
        if cluster < run.start || cluster >= run.end {
            continue;
        }
        let mut glyph = glyph.clone();
        glyph.x += *pen_x;
        glyph.advance += spacing;
        *pen_x += glyph.advance;
        out.push(glyph);
    }
}

/// Returns the end of a paragraph without its trailing paragraph separator.
fn paragraph_content_end(text: &str, end: usize) -> usize {
    let mut end = end.min(text.len());
    while end > 0 {
        let ch = text[..end].chars().next_back().expect("end > 0");
        if matches!(
            ch,
            '\n' | '\r' | '\u{001C}' | '\u{001D}' | '\u{001E}' | '\u{0085}' | '\u{2029}'
        ) {
            end -= ch.len_utf8();
        } else {
            break;
        }
    }
    end
}

/// Splits a bidi run into items that share a font and script.
fn itemize(fonts: &FontStack, text: &str, run: Range<usize>, allow_fallback: bool) -> Vec<Item> {
    let run_text = &text[run.clone()];
    let mut items = Vec::new();
    let mut item_start = run.start;
    let mut item_font: Option<FontId> = None;
    let mut item_script = harfrust::script::COMMON;
    // With a single face there is no fallback decision to make; punctuation always stays in it.
    let single_font = if fonts.len() == 1 {
        fonts.primary().map(|(id, _)| id)
    } else {
        None
    };

    for (offset, cluster) in run_text.grapheme_indices(true) {
        let absolute = run.start + offset;
        let Some(mut font) = single_font.or_else(|| fonts.select(cluster, allow_fallback)) else {
            return Vec::new();
        };
        let mut script = script_of(cluster);
        if single_font.is_none()
            && script == harfrust::script::COMMON
            && let Some(previous_font) = item_font
            && fonts.covers(previous_font, cluster)
        {
            // Common punctuation and spaces should stay in the surrounding font instead of
            // falling back to whatever face happens to be primary.
            font = previous_font;
        }
        if script == harfrust::script::COMMON && item_font == Some(font) {
            // Keep punctuation attached to the preceding run instead of shaping it in isolation.
            script = item_script;
        }

        if item_font == Some(font) && item_script == script {
            continue;
        }
        if let Some(previous_font) = item_font {
            items.push(Item {
                range: item_start..absolute,
                font: previous_font,
                script: item_script,
            });
        }
        item_start = absolute;
        item_font = Some(font);
        item_script = script;
    }

    if let Some(font) = item_font {
        items.push(Item {
            range: item_start..run.end,
            font,
            script: item_script,
        });
    }

    items
}

struct Item {
    range: Range<usize>,
    font: FontId,
    script: harfrust::Script,
}

fn shape_item(
    fonts: &FontStack,
    text: &str,
    item: Item,
    direction: Direction,
    style: &LayoutStyle,
) -> Option<ShapedItem> {
    if item.range.is_empty() {
        return None;
    }
    let face = fonts.get(item.font)?;
    let font = face.font().ok()?;

    let shaper = face.shaper_data().shaper(&font).build();
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(&text[item.range.clone()]);
    buffer.set_direction(direction);
    buffer.set_script(item.script);
    buffer.guess_segment_properties();

    let shaped = shaper.shape(buffer, &[]);
    let scale = if shaper.units_per_em() > 0 {
        style.font_size() / shaper.units_per_em() as f32
    } else {
        0.0
    };

    // HarfBuzz reports a cluster start for every glyph. A cluster ends where the next larger
    // start begins (or at the end of the shaped range), which lets text editors map byte indices
    // to caret positions without re-shaping prefixes.
    let mut cluster_starts: Vec<usize> = shaped
        .glyph_infos()
        .iter()
        .map(|info| item.range.start + info.cluster as usize)
        .collect();
    cluster_starts.sort_unstable();
    cluster_starts.dedup();

    let mut glyphs = Vec::with_capacity(shaped.len());
    for (info, position) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
        let glyph_id = GlyphId::new(info.glyph_id);
        let cluster_start = item.range.start + info.cluster as usize;
        let cluster_end = cluster_starts
            .iter()
            .copied()
            .find(|&start| start > cluster_start)
            .unwrap_or(item.range.end);
        glyphs.push(PositionedGlyph {
            font: item.font,
            glyph_id: info.glyph_id,
            // Relative to the item's pen; lines add their own offset while assembling.
            x: position.x_offset as f32 * scale,
            y: position.y_offset as f32 * scale,
            advance: position.x_advance as f32 * scale,
            color: face.has_color_glyph(glyph_id),
            cluster: cluster_start..cluster_end,
            rtl: direction == Direction::RightToLeft,
        });
    }

    Some(ShapedItem {
        range: item.range,
        glyphs,
    })
}

/// Maps a grapheme cluster to the script HarfBuzz should shape it with.
fn script_of(cluster: &str) -> harfrust::Script {
    // ASCII dominates UI text; skip the script table and short-name parse for it.
    if let Some(first) = cluster.chars().next()
        && first.is_ascii()
    {
        return if first.is_ascii_alphabetic() {
            harfrust::script::LATIN
        } else {
            harfrust::script::COMMON
        };
    }
    for ch in cluster.chars() {
        match UnicodeScript::from(ch) {
            UnicodeScript::Common | UnicodeScript::Inherited | UnicodeScript::Unknown => {}
            script => {
                return script
                    .short_name()
                    .parse()
                    .unwrap_or(harfrust::script::UNKNOWN);
            }
        }
    }
    harfrust::script::COMMON
}
