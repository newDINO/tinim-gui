//! The per-application GUI context and the output of one frame.

use tinim_paint::{PaintList, Painter, TextureId, TexturesDelta, Vec2};
use tinim_text::TextSystem;

use crate::input::RawInput;
use crate::style::Style;
use crate::text_cache::TextCache;
use crate::ui::Ui;

/// Owns the text system, painter and text cache.
///
/// Call [`Context::run`] once per frame. The returned [`Frame`] borrows the paint list, so it must
/// be rendered before the next frame starts.
pub struct Context {
    pub(crate) text_system: TextSystem,
    pub(crate) painter: Painter,
    pub(crate) text_cache: TextCache,
    style: Style,
    pub(crate) input: RawInput,
    size: Vec2,
    pub(crate) pixels_per_point: f32,
}

/// One frame's rendering data.
pub struct Frame<'a> {
    /// Tessellated geometry in physical pixels.
    pub paint: &'a PaintList,
    /// Glyph atlas uploads produced while building the frame.
    pub textures: TexturesDelta,
}

impl Context {
    /// Creates a context whose glyph atlas uses `texture_id`.
    ///
    /// Solid shapes sample the atlas's reserved white texel, so they batch with text instead of
    /// alternating between the atlas and the renderer's white texture. `TextureId::WHITE` is
    /// reserved by the renderer, so use another id.
    pub fn new(texture_id: TextureId) -> Self {
        Self {
            text_system: TextSystem::new(texture_id),
            painter: Painter::from_size(Vec2::ZERO, 1.0),
            text_cache: TextCache::default(),
            style: Style::default(),
            input: RawInput::default(),
            size: Vec2::ZERO,
            pixels_per_point: 1.0,
        }
    }

    /// Access the font stack. Add fonts before the first frame.
    ///
    /// Changing the font stack invalidates shaped layouts. Call
    /// [`Context::clear_text_cache`] if you add fonts after frames have already been built.
    pub fn text_system(&self) -> &TextSystem {
        &self.text_system
    }

    pub fn text_system_mut(&mut self) -> &mut TextSystem {
        &mut self.text_system
    }

    pub fn style(&self) -> &Style {
        &self.style
    }

    pub fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }

    /// Drops every cached shaped text layout.
    pub fn clear_text_cache(&mut self) {
        self.text_cache.clear();
    }

    /// Logical viewport size used by the current/last frame.
    pub fn size(&self) -> Vec2 {
        self.size
    }

    pub fn pixels_per_point(&self) -> f32 {
        self.pixels_per_point
    }

    pub fn input(&self) -> &RawInput {
        &self.input
    }

    /// Builds one frame.
    ///
    /// The closure runs immediately. `paint` is valid until the next call to `run`.
    pub fn run(
        &mut self,
        viewport: Vec2,
        pixels_per_point: f32,
        input: RawInput,
        build: impl FnOnce(&mut Ui<'_>),
    ) -> Frame<'_> {
        self.begin_frame(viewport, pixels_per_point, input);

        {
            let mut ui = Ui::new(self);
            build(&mut ui);
        }

        let paint = self.painter.finish();
        let textures = self.text_system.take_textures();
        Frame { paint, textures }
    }

    fn begin_frame(&mut self, viewport: Vec2, pixels_per_point: f32, input: RawInput) {
        self.size = sanitize_size(viewport);
        self.pixels_per_point = sanitize_scale(pixels_per_point);
        self.input = input;

        if (self.painter.pixels_per_point() - self.pixels_per_point).abs() > f32::EPSILON {
            self.painter = Painter::from_size(self.size, self.pixels_per_point);
        } else {
            self.painter.set_size(self.size.x, self.size.y);
        }

        // Solid geometry samples the atlas's reserved white texel so it batches with text. The
        // painter is recreated when the DPI scale changes, so reapply the setting every frame.
        self.text_system.ensure_atlas_texture();
        let solid_texture = self.text_system.atlas_texture_id();
        self.painter.set_solid_texture(solid_texture);

        self.painter.clear();
        self.painter.reset_clip_rect();
    }
}

fn sanitize_size(size: Vec2) -> Vec2 {
    if size.is_finite() {
        Vec2::new(size.x.max(0.0), size.y.max(0.0))
    } else {
        Vec2::ZERO
    }
}

fn sanitize_scale(scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_frame_allocates_the_atlas_even_without_text() {
        let mut context = Context::new(TextureId(1));
        let frame = context.run(Vec2::new(100.0, 100.0), 1.0, RawInput::default(), |_| {});

        assert!(frame.paint.draws.is_empty());
        assert_eq!(frame.textures.create, vec![(TextureId(1), [512, 512])]);
        assert_eq!(frame.textures.set.len(), 1);
        assert_eq!(frame.textures.set[0].1.pos, Some([0, 0]));
        assert_eq!(frame.textures.set[0].1.size, [1, 1]);
        assert_eq!(frame.textures.set[0].1.rgba, vec![255, 255, 255, 255]);

        drop(frame);
        let frame = context.run(Vec2::new(100.0, 100.0), 1.0, RawInput::default(), |_| {});
        assert!(frame.textures.is_empty(), "the texture already exists");
    }

    #[test]
    fn solid_texture_follows_the_atlas_after_a_dpi_change() {
        let mut context = Context::new(TextureId(1));
        context
            .text_system_mut()
            .add_font(font_test_data::AHEM.to_vec())
            .unwrap();
        let frame = context.run(Vec2::new(100.0, 100.0), 2.0, RawInput::default(), |ui| {
            ui.button("go");
        });
        assert_eq!(frame.paint.draws.len(), 1, "rect and text share the atlas");
        assert_eq!(frame.paint.draws[0].texture_id, TextureId(1));
    }
}
