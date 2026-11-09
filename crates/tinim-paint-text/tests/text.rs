//! End-to-end tests for the glyph atlas and mesh generation.

#[cfg(feature = "emoji-bitmap")]
use font_test_data::CBDT;
use font_test_data::{AHEM, COLRV0V1};
use skrifa::{FontRef, MetadataProvider, color::ColorGlyphFormat};
use tinim_paint::{Color32, ImageDelta, MeshSpace, Rect, TextureId, TexturesDelta, pos2, vec2};
use tinim_paint_text::TextSystem;
use tinim_text::TextStyle;

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

    let mesh = system.prepare(&layout, pos2(0.0, 0.0), Color32::BLACK, 1.0);
    assert_eq!(mesh.vertices.len(), 3 * 4);
    assert_eq!(mesh.indices.len(), 3 * 6);
    assert_eq!(mesh.texture_id, TextureId(1));
    assert_eq!(
        mesh.space,
        MeshSpace::Physical,
        "prepared text is already in physical pixels"
    );

    let textures = system.take_textures();
    assert_eq!(
        textures.create.len(),
        1,
        "the first frame creates the texture"
    );
    assert_eq!(textures.create[0].0, TextureId(1));
    assert_eq!(
        textures.set.len(),
        2,
        "the white texel and the new glyph are uploaded"
    );
    assert!(is_white_texel(&textures.set[0].1));
    assert!(
        textures.set[1].1.pos.is_some(),
        "glyphs are uploaded into their own rectangle"
    );

    // Preparing the same layout again must not re-upload anything.
    let mesh_again = system.prepare(&layout, pos2(0.0, 0.0), Color32::BLACK, 1.0);
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
    let mesh = system.prepare(&layout, pos2(0.0, 0.0), Color32::BLACK, 1.0);
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
    let mesh_again = system.prepare(&layout, pos2(0.0, 0.0), Color32::BLACK, 1.0);
    assert_eq!(mesh_again.vertices.len(), mesh.vertices.len());
    assert!(system.take_textures().is_empty());
}

#[test]
fn growth_recreates_an_existing_texture() {
    let mut system = TextSystem::with_atlas_size(TextureId(1), 64);
    system.add_font(AHEM.to_vec()).unwrap();

    let small = system.layout("a", &style(16.0), None);
    system.prepare(&small, pos2(0.0, 0.0), Color32::BLACK, 1.0);
    let textures = system.take_textures();
    assert_eq!(textures.create, vec![(TextureId(1), [64, 64])]);
    assert_eq!(textures.set.len(), 2);
    assert!(is_white_texel(&textures.set[0].1));

    let text = "abcdefghijklmnopqrstuvwxyz";
    let big = system.layout(text, &style(16.0), None);
    system.prepare(&big, pos2(0.0, 0.0), Color32::BLACK, 1.0);
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
    system.prepare(&first, pos2(0.0, 0.0), Color32::BLACK, 1.0);
    let textures = system.take_textures();
    assert_eq!(textures.create, vec![(TextureId(1), [64, 64])]);
    assert_eq!(textures.set.len(), 2);
    assert!(is_white_texel(&textures.set[0].1));

    // Next frame: a mesh is built before the overflowing layout is prepared.
    system.prepare(&first, pos2(0.0, 0.0), Color32::BLACK, 1.0);

    let text = "abcdefghijklmnopqrstuvwxyz";
    let big = system.layout(text, &style(16.0), None);
    let glyph_count = big.lines()[0].glyphs().len();
    let partial = system.prepare(&big, pos2(0.0, 0.0), Color32::BLACK, 1.0);
    assert!(
        partial.vertices.len() < glyph_count * 4,
        "the current frame skips glyphs that do not fit"
    );
    assert!(!partial.is_empty());

    // The earlier glyph is still cached, so its mesh can be rebuilt without growth.
    let again = system.prepare(&first, pos2(0.0, 0.0), Color32::BLACK, 1.0);
    assert_eq!(again.vertices.len(), 4);

    let textures = system.take_textures();
    assert!(
        textures.create.is_empty(),
        "growth is deferred to the start of the next frame"
    );

    // The next frame grows before building any mesh, so the full layout fits.
    let full = system.prepare(&big, pos2(0.0, 0.0), Color32::BLACK, 1.0);
    assert_eq!(full.vertices.len(), glyph_count * 4);
    let textures = system.take_textures();
    assert_eq!(textures.create.len(), 1);
    assert!(textures.create[0].1[0] > 64);
}

#[test]
fn glyphs_sit_on_the_baseline() {
    let mut system = TextSystem::new(TextureId(1));
    system
        .add_font(font_test_data::VAZIRMATN_VAR.to_vec())
        .unwrap();

    let layout = system.layout("A", &style(64.0), None);
    let baseline = layout.lines()[0].baseline();
    let mesh = system.prepare(&layout, pos2(0.0, 0.0), Color32::BLACK, 1.0);
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
fn prepare_uses_the_requested_color() {
    let mut system = TextSystem::new(TextureId(1));
    system.add_font(AHEM.to_vec()).unwrap();

    let layout = system.layout("A", &style(16.0), None);
    let red = Color32::from_rgb(255, 0, 0);
    let blue = Color32::from_rgb(0, 0, 255);

    let red_mesh = system.prepare(&layout, pos2(0.0, 0.0), red, 1.0);
    let blue_mesh = system.prepare(&layout, pos2(0.0, 0.0), blue, 1.0);

    assert!(!red_mesh.is_empty());
    assert!(
        red_mesh
            .vertices
            .iter()
            .all(|vertex| vertex.color == red.to_array())
    );
    assert!(
        blue_mesh
            .vertices
            .iter()
            .all(|vertex| vertex.color == blue.to_array())
    );
}

#[test]
fn second_upload_is_partial() {
    let mut system = TextSystem::new(TextureId(1));
    system.add_font(AHEM.to_vec()).unwrap();

    let first = system.layout("a", &style(16.0), None);
    system.prepare(&first, pos2(0.0, 0.0), Color32::BLACK, 1.0);
    let textures = system.take_textures();
    assert_eq!(textures.create.len(), 1);
    assert_eq!(textures.set.len(), 2);
    assert!(is_white_texel(&textures.set[0].1));
    assert!(textures.set[1].1.pos.is_some());

    let second = system.layout("z", &style(16.0), None);
    let mesh = system.prepare(&second, pos2(0.0, 0.0), Color32::BLACK, 1.0);
    assert!(!mesh.is_empty());
    let textures = system.take_textures();
    assert!(textures.create.is_empty(), "the texture already exists");
    assert_eq!(textures.set.len(), 1);
    assert!(textures.set[0].1.pos.is_some(), "later uploads are partial");
}

#[cfg(feature = "emoji-bitmap")]
#[test]
fn cbdt_bitmap_glyph_is_color() {
    let (ch, _) = first_bitmap_char(CBDT);
    let mut system = TextSystem::new(TextureId(1));
    system.add_font(CBDT.to_vec()).unwrap();

    let layout = system.layout(&ch.to_string(), &style(32.0), None);
    let mesh = system.prepare(&layout, pos2(0.0, 0.0), Color32::BLACK, 1.0);
    assert!(!mesh.is_empty());
    // Color glyphs ignore the text color and draw with white vertices.
    assert_eq!(mesh.vertices[0].color, Color32::WHITE.to_array());

    let textures = system.take_textures();
    let pixels = &glyph_delta(&textures).rgba;
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

    let mesh = system.prepare(&layout, pos2(0.0, 0.0), Color32::BLACK, 1.0);
    assert_eq!(mesh.vertices[0].color, Color32::WHITE.to_array());

    let textures = system.take_textures();
    let pixels = &glyph_delta(&textures).rgba;
    assert!(pixels.as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 0));
}

#[test]
fn ensure_atlas_texture_allocates_without_glyphs() {
    let mut system = TextSystem::new(TextureId(1));
    system.ensure_atlas_texture();

    let textures = system.take_textures();
    assert_eq!(textures.create, vec![(TextureId(1), [512, 512])]);
    assert_eq!(textures.set.len(), 1);
    assert_eq!(textures.set[0].0, TextureId(1));
    assert!(is_white_texel(&textures.set[0].1));
    assert!(
        system.take_textures().is_empty(),
        "the texture already exists"
    );
}

#[test]
fn glyphs_are_packed_after_the_white_texel() {
    let mut system = TextSystem::new(TextureId(1));
    system.add_font(AHEM.to_vec()).unwrap();
    let layout = system.layout("a", &style(16.0), None);
    system.prepare(&layout, pos2(0.0, 0.0), Color32::BLACK, 1.0);

    let textures = system.take_textures();
    assert!(is_white_texel(&textures.set[0].1));
    let [x, y] = glyph_delta(&textures)
        .pos
        .expect("glyph uploads are partial");
    assert!(
        x >= 2 || y >= 2,
        "glyph at ({x}, {y}) overlaps the white texel"
    );
}

#[test]
fn growth_reuploads_the_white_texel() {
    let mut system = TextSystem::with_atlas_size(TextureId(1), 64);
    system.add_font(AHEM.to_vec()).unwrap();

    let small = system.layout("a", &style(16.0), None);
    system.prepare(&small, pos2(0.0, 0.0), Color32::BLACK, 1.0);
    system.take_textures();

    let big = system.layout("abcdefghijklmnopqrstuvwxyz", &style(16.0), None);
    system.prepare(&big, pos2(0.0, 0.0), Color32::BLACK, 1.0);
    let textures = system.take_textures();
    assert_eq!(textures.create.len(), 1);
    assert!(
        textures.set.iter().any(|(_, image)| is_white_texel(image)),
        "a replaced texture is zero-initialized, so white must be uploaded again"
    );
}

#[test]
fn reset_re_reserves_the_white_texel() {
    let mut system = TextSystem::new(TextureId(1));
    system.add_font(AHEM.to_vec()).unwrap();
    let layout = system.layout("a", &style(16.0), None);
    system.prepare(&layout, pos2(0.0, 0.0), Color32::BLACK, 1.0);
    let first = system.take_textures();
    let first_pos = glyph_delta(&first).pos;

    system.reset_atlas();
    system.prepare(&layout, pos2(0.0, 0.0), Color32::BLACK, 1.0);
    let textures = system.take_textures();
    assert!(textures.create.is_empty(), "the texture still exists");

    let pos = glyph_delta(&textures).pos;
    assert_eq!(pos, first_pos);
    let [x, y] = pos.expect("glyph uploads are partial");
    assert!(
        x >= 2 || y >= 2,
        "glyph at ({x}, {y}) overlaps the white texel"
    );
}

#[test]
fn atlas_mesh_draws_the_whole_texture() {
    let mut system = TextSystem::new(TextureId(1));
    system.ensure_atlas_texture();

    let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(100.0, 50.0));
    let mesh = system.atlas_mesh(rect, Color32::WHITE);

    assert_eq!(mesh.texture_id, TextureId(1));
    assert_eq!(mesh.space, MeshSpace::Logical);
    assert_eq!(mesh.vertices.len(), 4);
    assert_eq!(mesh.vertices[0].pos, [10.0, 20.0]);
    assert_eq!(mesh.vertices[0].uv, [0, 0]);
    assert_eq!(mesh.vertices[2].pos, [110.0, 70.0]);
    assert_eq!(mesh.vertices[2].uv, [65535, 65535]);
    assert_eq!(system.atlas_size(), [512, 512]);
}

#[test]
fn empty_text_has_no_texture() {
    let mut system = TextSystem::new(TextureId(1));
    system.add_font(AHEM.to_vec()).unwrap();
    let layout = system.layout("", &style(16.0), None);
    assert_eq!(layout.line_count(), 0);
    assert!(
        system
            .prepare(&layout, pos2(0.0, 0.0), Color32::BLACK, 1.0)
            .is_empty()
    );
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

fn is_white_texel(delta: &ImageDelta) -> bool {
    delta.pos == Some([0, 0])
        && delta.size == [1, 1]
        && delta.rgba.as_slice() == [255, 255, 255, 255]
}

fn glyph_delta(textures: &TexturesDelta) -> &ImageDelta {
    textures
        .set
        .iter()
        .map(|(_, delta)| delta)
        .find(|delta| !is_white_texel(delta))
        .expect("the frame contains a glyph upload")
}
