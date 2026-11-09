//! End-to-end tests using fonts from `font-test-data`.
//!
//! The test container has no system fonts, so everything here is loaded explicitly.

#[cfg(any(feature = "fallback", feature = "emoji-bitmap"))]
use font_test_data::CBDT;
use font_test_data::{AHEM, COLRV0V1, VAZIRMATN_VAR};
use skrifa::{FontRef, MetadataProvider, color::ColorGlyphFormat};
use tinim_text::{TextContext, TextDirection, TextStyle, rasterize};

fn style(size: f32) -> TextStyle {
    TextStyle {
        font_size: size,
        ..Default::default()
    }
}

fn context(font: &[u8]) -> TextContext {
    let mut text = TextContext::new();
    text.add_font(font.to_vec()).unwrap();
    text
}

#[test]
fn ahem_layout() {
    let text = context(AHEM);

    let layout = text.layout("aaa", &style(16.0), None);
    assert_eq!(layout.line_count(), 1);
    assert_eq!(layout.lines()[0].glyphs().len(), 3);
    assert!((layout.width() - 48.0).abs() < 0.01);
    assert!((layout.height() - layout.lines()[0].baseline()).abs() < 40.0);
}

#[test]
fn rasterize_ahem_glyph() {
    let text = context(AHEM);
    let layout = text.layout("A", &style(16.0), None);
    let glyph = &layout.lines()[0].glyphs()[0];

    let bitmap = rasterize(
        text.font(glyph.font()).unwrap(),
        glyph.glyph_id(),
        layout.style().font_size(),
    )
    .expect("AHEM has an outline for A");

    assert!(bitmap.width > 0);
    assert!(bitmap.height > 0);
    assert_eq!(
        bitmap.pixels.len(),
        bitmap.width as usize * bitmap.height as usize * 4
    );
    assert!(!bitmap.color, "AHEM is a monochrome outline font");
    assert!(
        bitmap.pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 0),
        "the rasterized glyph has coverage"
    );
}

#[test]
fn rasterize_missing_glyph_is_none() {
    let text = context(AHEM);
    let (_, face) = text.fonts().iter().next().unwrap();
    // Far beyond the glyph count of any test font.
    assert!(rasterize(face, 0xFFFF, 16.0).is_none());
}

#[test]
fn wrapping_uses_line_breaks() {
    let text = context(AHEM);

    // AHEM's advance is one em, so two glyphs fit in 40 points and three do not.
    let layout = text.layout("aaaa", &style(16.0), Some(40.0));
    assert_eq!(layout.line_count(), 2);
    assert!((layout.width() - 32.0).abs() < 0.01);

    // Even a width smaller than one glyph makes progress by breaking between graphemes.
    let narrow = text.layout("aaaa", &style(16.0), Some(1.0));
    assert_eq!(narrow.line_count(), 4);
    for line in narrow.lines() {
        assert_eq!(line.glyphs().len(), 1);
    }
}

#[test]
fn letter_spacing_is_not_added_after_the_last_glyph() {
    let text = context(AHEM);

    // AHEM advances one em per glyph. With 1pt spacing, "aaa" is 3 * 16 + 2 * 1 = 50 points.
    let mut spaced = style(16.0);
    spaced.letter_spacing = 1.0;
    let layout = text.layout("aaa", &spaced, None);
    assert_eq!(layout.line_count(), 1);
    assert!((layout.width() - 50.0).abs() < 0.01, "{}", layout.width());
    assert!((layout.lines()[0].width() - 50.0).abs() < 0.01);

    // The trailing spacing must not push the text onto a second line.
    let wrapped = text.layout("aaa", &spaced, Some(50.0));
    assert_eq!(
        wrapped.line_count(),
        1,
        "trailing spacing must not cause a wrap"
    );
}

#[test]
fn explicit_newlines_create_empty_lines() {
    let text = context(AHEM);

    let layout = text.layout("a\n\nb", &style(16.0), None);
    assert_eq!(layout.line_count(), 3);
    assert_eq!(layout.lines()[1].glyphs().len(), 0);
    assert_eq!(layout.lines()[2].glyphs().len(), 1);
}

#[test]
fn default_line_height_includes_the_descent() {
    let text = context(AHEM);

    // AHEM's ascent and descent are 800 and 200 units in a 1000 unit em, so lines are one em
    // apart at 16 points. Regression test: the descent must not be subtracted from the ascent.
    let layout = text.layout("a\na", &style(16.0), None);
    assert_eq!(layout.line_count(), 2);
    let first = layout.lines()[0].baseline();
    let second = layout.lines()[1].baseline();
    assert!((second - first - 16.0).abs() < 0.01, "{first} -> {second}");
    assert!((layout.height() - 32.0).abs() < 0.01, "{}", layout.height());
}

#[test]
fn rtl_reverses_glyph_order() {
    let text = context(VAZIRMATN_VAR);

    // The first font-test-data face has distinct glyphs for A (1) and À (2). RLO forces an RTL
    // paragraph, so the visual order must be reversed.
    let layout = text.layout("\u{202E}A\u{00C0}", &style(24.0), None);
    let glyph_ids: Vec<u32> = layout.lines()[0]
        .glyphs()
        .iter()
        .map(|glyph| glyph.glyph_id())
        .collect();
    assert_eq!(glyph_ids, [2, 1]);
}

#[cfg(feature = "fallback")]
#[test]
fn rtl_fallback_items_are_reversed() {
    let mut text = TextContext::new();
    let primary = text.add_font(AHEM.to_vec()).unwrap();
    let fallback = text.add_font(CBDT.to_vec()).unwrap();

    // Both characters are neutral, so the RLO makes them one RTL run, but they are covered by
    // different faces and therefore shaped as two items. The logical first item ("!") must be
    // drawn to the right of the logical last item ("♢").
    let layout = text.layout("\u{202E}!\u{2662}", &style(24.0), None);
    let glyphs: Vec<_> = layout.lines()[0]
        .glyphs()
        .iter()
        .filter(|glyph| glyph.advance() > 0.0)
        .collect();
    assert_eq!(glyphs.len(), 2);
    assert_eq!(glyphs[0].font(), fallback);
    assert_eq!(glyphs[1].font(), primary);
}

#[cfg(feature = "fallback")]
#[test]
fn fallback_picks_a_covering_face() {
    let mut text = TextContext::new();
    let primary = text.add_font(AHEM.to_vec()).unwrap();
    let fallback = text.add_font(VAZIRMATN_VAR.to_vec()).unwrap();

    let primary_face = text.font(primary).unwrap();
    let fallback_face = text.font(fallback).unwrap();
    let Some(ch) = "À"
        .chars()
        .find(|&ch| !primary_face.has_char(ch) && fallback_face.has_char(ch))
    else {
        return;
    };

    let sample = format!("A{ch}");
    let layout = text.layout(&sample, &style(20.0), None);
    let fonts: Vec<_> = layout.lines()[0]
        .glyphs()
        .iter()
        .map(|glyph| glyph.font())
        .collect();
    assert!(
        fonts.contains(&fallback),
        "the fallback char used the fallback"
    );
    assert!(fonts.contains(&primary), "the Latin char used the primary");
}

#[cfg(feature = "fallback")]
#[test]
fn common_punctuation_follows_the_previous_font() {
    let mut text = TextContext::new();
    let _primary = text.add_font(AHEM.to_vec()).unwrap();
    let fallback = text.add_font(CBDT.to_vec()).unwrap();

    // AHEM covers the space, but the preceding glyph is only in CBDT. The space belongs to the
    // surrounding run and must not switch back to the primary face.
    let layout = text.layout("♢ ♢", &style(24.0), None);
    let fonts: Vec<_> = layout.lines()[0]
        .glyphs()
        .iter()
        .map(|glyph| glyph.font())
        .collect();
    assert_eq!(fonts, [fallback, fallback, fallback]);
}

#[test]
fn explicit_direction_changes_visual_order() {
    let text = context(AHEM);

    let layout = |direction| {
        text.layout(
            "A?",
            &TextStyle {
                direction,
                ..style(16.0)
            },
            None,
        )
    };
    let ids = |layout: tinim_text::TextLayout| -> Vec<u32> {
        layout.lines()[0]
            .glyphs()
            .iter()
            .map(|glyph| glyph.glyph_id())
            .collect()
    };

    let ltr = ids(layout(TextDirection::LeftToRight));
    let rtl = ids(layout(TextDirection::RightToLeft));
    assert_eq!(ltr.len(), 2);
    assert_eq!(rtl, ltr.iter().rev().copied().collect::<Vec<_>>());
}

#[cfg(feature = "emoji-bitmap")]
#[test]
fn cbdt_bitmap_glyph_is_color() {
    let (ch, glyph_id) = first_bitmap_char(CBDT);
    let text = context(CBDT);

    let layout = text.layout(&ch.to_string(), &style(32.0), None);
    assert_eq!(layout.lines()[0].glyphs().len(), 1);
    assert!(layout.lines()[0].glyphs()[0].is_color());

    let bitmap = rasterize(
        text.font(layout.lines()[0].glyphs()[0].font()).unwrap(),
        glyph_id.to_u32(),
        32.0,
    )
    .expect("the bitmap strike rasterizes");
    assert!(bitmap.color);
    let pixels = bitmap.pixels.as_chunks::<4>().0;
    assert!(pixels.iter().any(|pixel| pixel[3] != 0));
    assert!(
        pixels
            .iter()
            .any(|pixel| pixel[0] != pixel[1] || pixel[1] != pixel[2]),
        "the bitmap should contain actual colors"
    );
}

#[test]
fn colrv0_glyph_is_composited() {
    let font = FontRef::new(COLRV0V1).unwrap();
    let mut charmap = font.charmap().mappings();
    let Some((codepoint, glyph_id)) = charmap.find_map(|(codepoint, glyph_id)| {
        font.color_glyphs()
            .get_with_format(glyph_id, ColorGlyphFormat::ColrV0)
            .is_some()
            .then_some((codepoint, glyph_id))
    }) else {
        return;
    };
    let Some(ch) = char::from_u32(codepoint) else {
        return;
    };

    let text = context(COLRV0V1);
    let layout = text.layout(&ch.to_string(), &style(64.0), None);
    assert!(layout.lines()[0].glyphs()[0].is_color());

    let bitmap = rasterize(
        text.font(layout.lines()[0].glyphs()[0].font()).unwrap(),
        glyph_id.to_u32(),
        64.0,
    )
    .expect("the COLRv0 glyph rasterizes");
    assert!(bitmap.color);
    assert!(
        bitmap
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] != 0)
    );
}

#[test]
fn empty_text_has_no_lines() {
    let text = context(AHEM);
    let layout = text.layout("", &style(16.0), None);
    assert_eq!(layout.line_count(), 0);
    assert_eq!(layout.size(), tinim_text::Vec2::ZERO);
}

#[cfg(feature = "emoji-bitmap")]
fn first_bitmap_char(font_data: &[u8]) -> (char, skrifa::GlyphId) {
    let font = FontRef::new(font_data).unwrap();
    let strikes = font.bitmap_strikes();
    font.charmap()
        .mappings()
        .find_map(|(codepoint, glyph_id)| {
            if strikes.iter().any(|strike| strike.get(glyph_id).is_some()) {
                char::from_u32(codepoint).map(|ch| (ch, glyph_id))
            } else {
                None
            }
        })
        .expect("test font has at least one bitmap glyph")
}
