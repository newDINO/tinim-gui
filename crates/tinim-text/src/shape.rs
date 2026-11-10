//! Bidi resolution, itemization and OpenType shaping.

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

/// Shapes one line of text in visual order and returns the glyphs and the line width.
///
/// `line` must be a byte range inside `paragraph`. RTL runs are already reversed by HarfBuzz,
/// so glyphs are emitted from left to right and their advances always add to `pen_x`.
pub(crate) fn shape_line(
    fonts: &FontStack,
    text: &str,
    bidi: &BidiInfo<'_>,
    paragraph: &ParagraphInfo,
    line: Range<usize>,
    style: &LayoutStyle,
    allow_fallback: bool,
) -> (Vec<PositionedGlyph>, f32) {
    if line.is_empty() || line.start >= line.end || line.end > text.len() {
        return (Vec::new(), 0.0);
    }

    let (levels, runs) = bidi.visual_runs(paragraph, line);
    let mut glyphs = Vec::new();
    let mut pen_x = 0.0;

    for run in runs {
        if run.is_empty() {
            continue;
        }
        let direction = if levels[run.start].is_rtl() {
            Direction::RightToLeft
        } else {
            Direction::LeftToRight
        };
        shape_visual_run(
            fonts,
            text,
            run,
            direction,
            style,
            allow_fallback,
            &mut pen_x,
            &mut glyphs,
        );
    }

    // Letter spacing goes between glyphs, not after the last one. The loop above adds it to
    // every glyph, so remove the trailing instance from the final advance and the line width.
    if let Some(last) = glyphs.last_mut() {
        let spacing = style.letter_spacing();
        last.advance -= spacing;
        pen_x -= spacing;
    }

    (glyphs, pen_x)
}

#[allow(clippy::too_many_arguments)]
fn shape_visual_run(
    fonts: &FontStack,
    text: &str,
    run: Range<usize>,
    direction: Direction,
    style: &LayoutStyle,
    allow_fallback: bool,
    pen_x: &mut f32,
    out: &mut Vec<PositionedGlyph>,
) {
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
            return;
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

    // Each item is shaped in visual order internally. For RTL runs the *items* themselves are in
    // logical order, so they have to be appended in reverse to get a visual left-to-right line.
    if direction == Direction::RightToLeft {
        items.reverse();
    }
    for item in items {
        shape_item(
            fonts,
            text,
            item.range,
            item.font,
            direction,
            item.script,
            style,
            pen_x,
            out,
        );
    }
}

struct Item {
    range: Range<usize>,
    font: FontId,
    script: harfrust::Script,
}

#[allow(clippy::too_many_arguments)]
fn shape_item(
    fonts: &FontStack,
    text: &str,
    range: Range<usize>,
    font_id: FontId,
    direction: Direction,
    script: harfrust::Script,
    style: &LayoutStyle,
    pen_x: &mut f32,
    out: &mut Vec<PositionedGlyph>,
) {
    if range.is_empty() {
        return;
    }
    let Some(face) = fonts.get(font_id) else {
        return;
    };
    let Ok(font) = face.font() else {
        return;
    };

    let shaper = face.shaper_data().shaper(&font).build();
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(&text[range]);
    buffer.set_direction(direction);
    buffer.set_script(script);
    buffer.guess_segment_properties();

    let shaped = shaper.shape(buffer, &[]);
    let scale = if shaper.units_per_em() > 0 {
        style.font_size() / shaper.units_per_em() as f32
    } else {
        0.0
    };
    let spacing = style.letter_spacing();

    for (info, position) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
        let glyph_id = GlyphId::new(info.glyph_id);
        let color = face.has_color_glyph(glyph_id);
        let advance = position.x_advance as f32 * scale;
        out.push(PositionedGlyph {
            font: font_id,
            glyph_id: info.glyph_id,
            x: *pen_x + position.x_offset as f32 * scale,
            y: position.y_offset as f32 * scale,
            advance: advance + spacing,
            color,
        });
        *pen_x += advance + spacing;
    }
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
