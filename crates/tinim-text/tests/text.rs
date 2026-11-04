//! End-to-end tests using fonts from `font-test-data`.
//!
//! The test container has no system fonts, so everything here is loaded explicitly.

#[cfg(any(feature = "fallback", feature = "emoji-bitmap"))]
use font_test_data::CBDT;
use font_test_data::{AHEM, COLRV0V1, VAZIRMATN_VAR};
use skrifa::{FontRef, MetadataProvider, color::ColorGlyphFormat};
use tinim_paint::{Color32, TextureId, pos2};
use tinim_text::{TextDirection, TextStyle, TextSystem};

fn style(size: f32) -> TextStyle {
    TextStyle {
        font_size: size,
        ..Default::default()
    }
}

#[test]
fn ahem_layout_and_atlas_upload() {
    let mut system = TextSystem::new(TextureId(1));
    system.add_font(AHEM.to_vec()).unwrap();

    let layout = system.layout("aaa", &style(16.0), None);
    assert_eq!(layout.line_count(), 1);
    assert_eq!(layout.lines()[0].glyphs().len(), 3);
    assert!((layout.width() - 48.0).abs() < 0.01);
    assert!((layout.height() - layout.lines()[0].baseline()).abs() < 40.0);

    let mesh = system.prepare(&layout, pos2(0.0, 0.0), 1.0);
    assert_eq!(mesh.vertices.len(), 3 * 4);
    assert_eq!(mesh.indices.len(), 3 * 6);
    assert_eq!(mesh.texture_id, TextureId(1));

    let textures = system.take_textures();
    assert_eq!(
        textures.create.len(),
        1,
        "the first frame creates the texture"
    );
    assert_eq!(textures.create[0].0, TextureId(1));
    assert_eq!(textures.set.len(), 1, "only the new glyph is uploaded");
    assert!(
        textures.set[0].1.pos.is_some(),
        "glyphs are uploaded into their own rectangle"
    );

    // Preparing the same layout again must not re-upload anything.
    let mesh_again = system.prepare(&layout, pos2(0.0, 0.0), 1.0);
    assert_eq!(mesh_again.vertices.len(), mesh.vertices.len());
    assert!(system.take_textures().is_empty());
}

#[test]
fn atlas_grows_instead_of_resetting() {
    let mut system = TextSystem::with_atlas_size(TextureId(1), 64);
    system.add_font(AHEM.to_vec()).unwrap();

    let text = "abcdefghijklmnopqrstuvwxyz";
    let layout = system.layout(text, &style(16.0), None);
    let glyph_count = layout.lines()[0].glyphs().len();
    let mesh = system.prepare(&layout, pos2(0.0, 0.0), 1.0);
    assert_eq!(mesh.vertices.len(), glyph_count * 4);

    let textures = system.take_textures();
    assert_eq!(textures.create.len(), 1);
    let [width, height] = textures.create[0].1;
    assert!(width > 64, "the atlas should grow beyond its initial size");
    assert_eq!(width, height);
    assert!(
        textures.set.len() >= glyph_count,
        "every cached glyph is re-uploaded after growth"
    );

    // Growth re-packs the cache, so the same layout must not upload anything again.
    let mesh_again = system.prepare(&layout, pos2(0.0, 0.0), 1.0);
    assert_eq!(mesh_again.vertices.len(), mesh.vertices.len());
    assert!(system.take_textures().is_empty());
}

#[test]
fn growth_recreates_an_existing_texture() {
    let mut system = TextSystem::with_atlas_size(TextureId(1), 64);
    system.add_font(AHEM.to_vec()).unwrap();

    let small = system.layout("a", &style(16.0), None);
    system.prepare(&small, pos2(0.0, 0.0), 1.0);
    let textures = system.take_textures();
    assert_eq!(textures.create, vec![(TextureId(1), [64, 64])]);
    assert_eq!(textures.set.len(), 1);

    let text = "abcdefghijklmnopqrstuvwxyz";
    let big = system.layout(text, &style(16.0), None);
    system.prepare(&big, pos2(0.0, 0.0), 1.0);
    let textures = system.take_textures();
    assert_eq!(textures.create.len(), 1);
    assert!(
        textures.create[0].1[0] > 64,
        "the renderer is told to replace the texture"
    );
    assert!(textures.set.len() > 1);
}

#[test]
fn growth_is_deferred_when_a_mesh_already_exists() {
    let mut system = TextSystem::with_atlas_size(TextureId(1), 64);
    system.add_font(AHEM.to_vec()).unwrap();

    // First frame: the texture is allocated at the initial size.
    let first = system.layout("a", &style(16.0), None);
    system.prepare(&first, pos2(0.0, 0.0), 1.0);
    let textures = system.take_textures();
    assert_eq!(textures.create, vec![(TextureId(1), [64, 64])]);
    assert_eq!(textures.set.len(), 1);

    // Next frame: a mesh is built before the overflowing layout is prepared.
    system.prepare(&first, pos2(0.0, 0.0), 1.0);

    let text = "abcdefghijklmnopqrstuvwxyz";
    let big = system.layout(text, &style(16.0), None);
    let glyph_count = big.lines()[0].glyphs().len();
    let partial = system.prepare(&big, pos2(0.0, 0.0), 1.0);
    assert!(
        partial.vertices.len() < glyph_count * 4,
        "the current frame skips glyphs that do not fit"
    );
    assert!(!partial.is_empty());

    // The earlier glyph is still cached, so its mesh can be rebuilt without growth.
    let again = system.prepare(&first, pos2(0.0, 0.0), 1.0);
    assert_eq!(again.vertices.len(), 4);

    let textures = system.take_textures();
    assert!(
        textures.create.is_empty(),
        "growth is deferred to the start of the next frame"
    );

    // The next frame grows before building any mesh, so the full layout fits.
    let full = system.prepare(&big, pos2(0.0, 0.0), 1.0);
    assert_eq!(full.vertices.len(), glyph_count * 4);
    let textures = system.take_textures();
    assert_eq!(textures.create.len(), 1);
    assert!(textures.create[0].1[0] > 64);
}

#[test]
fn glyphs_sit_on_the_baseline() {
    let mut system = TextSystem::new(TextureId(1));
    system.add_font(VAZIRMATN_VAR.to_vec()).unwrap();

    let layout = system.layout("A", &style(64.0), None);
    let baseline = layout.lines()[0].baseline();
    let mesh = system.prepare(&layout, pos2(0.0, 0.0), 1.0);
    let top = mesh
        .vertices
        .iter()
        .map(|vertex| vertex.pos[1])
        .fold(f32::INFINITY, f32::min);
    let bottom = mesh
        .vertices
        .iter()
        .map(|vertex| vertex.pos[1])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        bottom <= baseline + 0.5,
        "glyph bottom {bottom} hangs below the baseline {baseline}"
    );
    assert!(
        top < baseline - 10.0,
        "glyph top {top} is not above the baseline {baseline}"
    );
}

#[test]
fn wrapping_uses_line_breaks() {
    let mut system = TextSystem::new(TextureId(1));
    system.add_font(AHEM.to_vec()).unwrap();

    // AHEM's advance is one em, so two glyphs fit in 40 points and three do not.
    let layout = system.layout("aaaa", &style(16.0), Some(40.0));
    assert_eq!(layout.line_count(), 2);
    assert!((layout.width() - 32.0).abs() < 0.01);

    // Even a width smaller than one glyph makes progress by breaking between graphemes.
    let narrow = system.layout("aaaa", &style(16.0), Some(1.0));
    assert_eq!(narrow.line_count(), 4);
    for line in narrow.lines() {
        assert_eq!(line.glyphs().len(), 1);
    }
}

#[test]
fn explicit_newlines_create_empty_lines() {
    let mut system = TextSystem::new(TextureId(1));
    system.add_font(AHEM.to_vec()).unwrap();

    let layout = system.layout("a\n\nb", &style(16.0), None);
    assert_eq!(layout.line_count(), 3);
    assert_eq!(layout.lines()[1].glyphs().len(), 0);
    assert_eq!(layout.lines()[2].glyphs().len(), 1);
}

#[test]
fn second_upload_is_partial() {
    let mut system = TextSystem::new(TextureId(1));
    system.add_font(AHEM.to_vec()).unwrap();

    let first = system.layout("a", &style(16.0), None);
    system.prepare(&first, pos2(0.0, 0.0), 1.0);
    let textures = system.take_textures();
    assert_eq!(textures.create.len(), 1);
    assert!(textures.set[0].1.pos.is_some());

    let second = system.layout("z", &style(16.0), None);
    let mesh = system.prepare(&second, pos2(0.0, 0.0), 1.0);
    assert!(!mesh.is_empty());
    let textures = system.take_textures();
    assert!(textures.create.is_empty(), "the texture already exists");
    assert_eq!(textures.set.len(), 1);
    assert!(textures.set[0].1.pos.is_some(), "later uploads are partial");
}

#[test]
fn rtl_reverses_glyph_order() {
    let mut system = TextSystem::new(TextureId(1));
    system.add_font(VAZIRMATN_VAR.to_vec()).unwrap();

    // The first font-test-data face has distinct glyphs for A (1) and À (2). RLO forces an RTL
    // paragraph, so the visual order must be reversed.
    let layout = system.layout("\u{202E}A\u{00C0}", &style(24.0), None);
    let glyph_ids: Vec<u32> = layout.lines()[0]
        .glyphs()
        .iter()
        .map(|glyph| glyph.glyph_id())
        .collect();
    assert_eq!(glyph_ids, [2, 1]);

    let mesh = system.prepare(&layout, pos2(10.0, 20.0), 2.0);
    assert!(!mesh.is_empty());
    system.take_textures();
}

#[cfg(feature = "fallback")]
#[test]
fn rtl_fallback_items_are_reversed() {
    let mut system = TextSystem::new(TextureId(1));
    let primary = system.add_font(AHEM.to_vec()).unwrap();
    let fallback = system.add_font(CBDT.to_vec()).unwrap();

    // Both characters are neutral, so the RLO makes them one RTL run, but they are covered by
    // different faces and therefore shaped as two items. The logical first item ("!") must be
    // drawn to the right of the logical last item ("♢").
    let layout = system.layout("\u{202E}!\u{2662}", &style(24.0), None);
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
    let mut system = TextSystem::new(TextureId(1));
    let primary = system.add_font(AHEM.to_vec()).unwrap();
    let fallback = system.add_font(VAZIRMATN_VAR.to_vec()).unwrap();

    let primary_face = system.font(primary).unwrap();
    let fallback_face = system.font(fallback).unwrap();
    let Some(ch) = "À"
        .chars()
        .find(|&ch| !primary_face.has_char(ch) && fallback_face.has_char(ch))
    else {
        return;
    };

    let text = format!("A{ch}");
    let layout = system.layout(&text, &style(20.0), None);
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
    let mut system = TextSystem::new(TextureId(1));
    let _primary = system.add_font(AHEM.to_vec()).unwrap();
    let fallback = system.add_font(CBDT.to_vec()).unwrap();

    // AHEM covers the space, but the preceding glyph is only in CBDT. The space belongs to the
    // surrounding run and must not switch back to the primary face.
    let layout = system.layout("♢ ♢", &style(24.0), None);
    let fonts: Vec<_> = layout.lines()[0]
        .glyphs()
        .iter()
        .map(|glyph| glyph.font())
        .collect();
    assert_eq!(fonts, [fallback, fallback, fallback]);
}

#[test]
fn explicit_direction_changes_visual_order() {
    let mut system = TextSystem::new(TextureId(1));
    system.add_font(AHEM.to_vec()).unwrap();

    let layout = |direction| {
        system.layout(
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
    let (ch, _) = first_bitmap_char(CBDT);
    let mut system = TextSystem::new(TextureId(1));
    system.add_font(CBDT.to_vec()).unwrap();

    let layout = system.layout(&ch.to_string(), &style(32.0), None);
    assert_eq!(layout.lines()[0].glyphs().len(), 1);
    assert!(layout.lines()[0].glyphs()[0].is_color());

    let mesh = system.prepare(&layout, pos2(0.0, 0.0), 1.0);
    assert!(!mesh.is_empty());
    // Color glyphs ignore the text color and draw with white vertices.
    assert_eq!(mesh.vertices[0].color, Color32::WHITE.to_array());

    let textures = system.take_textures();
    let pixels = &textures.set[0].1.rgba;
    let pixels = pixels.as_chunks::<4>().0;
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
    let Some((codepoint, _)) = charmap.find_map(|(codepoint, glyph_id)| {
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

    let mut system = TextSystem::new(TextureId(1));
    system.add_font(COLRV0V1.to_vec()).unwrap();
    let layout = system.layout(&ch.to_string(), &style(64.0), None);
    assert!(layout.lines()[0].glyphs()[0].is_color());

    let mesh = system.prepare(&layout, pos2(0.0, 0.0), 1.0);
    assert_eq!(mesh.vertices[0].color, Color32::WHITE.to_array());

    let textures = system.take_textures();
    let pixels = &textures.set[0].1.rgba;
    assert!(pixels.as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 0));
}

#[test]
fn empty_text_has_no_lines() {
    let mut system = TextSystem::new(TextureId(1));
    system.add_font(AHEM.to_vec()).unwrap();
    let layout = system.layout("", &style(16.0), None);
    assert_eq!(layout.line_count(), 0);
    assert_eq!(layout.size(), tinim_paint::Vec2::ZERO);
    assert!(system.prepare(&layout, pos2(0.0, 0.0), 1.0).is_empty());
    assert!(
        system.take_textures().is_empty(),
        "no glyph was rasterized, so no texture is allocated"
    );
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
