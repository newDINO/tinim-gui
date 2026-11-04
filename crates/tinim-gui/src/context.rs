//! The per-application GUI context and the output of one frame.

use std::sync::Arc;

use tinim_paint::{Mesh, PaintList, Painter, Pos2, TextureId, TexturesDelta, Vec2};
use tinim_text::{TextLayout, TextStyle, TextSystem};

use crate::input::RawInput;
use crate::style::Style;
use crate::text_cache::TextCache;
use crate::ui::Ui;

/// Owns the text system, painter and text cache.
///
/// Call [`Context::run`] once per frame. The returned [`Frame`] borrows the paint list, so it must
/// be rendered before the next frame starts.
pub struct Context {
    text: TextSystem,
    painter: Painter,
    text_cache: TextCache,
    style: Style,
    input: RawInput,
    size: Vec2,
    pixels_per_point: f32,
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
    /// `TextureId::WHITE` is reserved by the renderer, so use another id.
    pub fn new(texture_id: TextureId) -> Self {
        Self {
            text: TextSystem::new(texture_id),
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
    pub fn text(&self) -> &TextSystem {
        &self.text
    }

    pub fn text_mut(&mut self) -> &mut TextSystem {
        &mut self.text
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
        let textures = self.text.take_textures();
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

        self.painter.clear();
        self.painter.reset_clip_rect();
    }

    pub(crate) fn text_layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f32>,
    ) -> Arc<TextLayout> {
        self.text_cache
            .layout(&mut self.text, text, style, max_width)
    }

    pub(crate) fn prepare_text(&mut self, layout: &TextLayout, origin: Pos2) -> Mesh {
        self.text.prepare(layout, origin, self.pixels_per_point)
    }

    pub(crate) fn painter_mut(&mut self) -> &mut Painter {
        &mut self.painter
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
