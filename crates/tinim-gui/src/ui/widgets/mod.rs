//! Leaf widgets: labels, separators, buttons, toggles, sliders, number inputs and text editing.

use tinim_paint::{Color32, Painter, Pos2, Rect};
use tinim_paint_text::TextSystem;
use tinim_text::TextLayout;

use super::Ui;
use super::layout::{Align2, allocate_space, available_size, pointer_over};
use crate::input::RawInput;

mod button;
mod image;
mod number_input;
mod separator;
mod slider;
mod text_edit;
mod toggle;

pub use image::{ImageFit, ImageOptions};
pub use number_input::{NumberInputResponse, NumberInputState};
pub use text_edit::{TextEditResponse, TextEditState};

/// Pointer state shared by interactive widgets.
#[derive(Clone, Copy, Debug)]
pub(super) struct Interaction {
    pub hovered: bool,
    pub pressed: bool,
    pub down: bool,
}

/// Hit-tests `rect` once and derives the press edges every interactive widget reports.
pub(super) fn interaction(clip: Rect, input: &RawInput, rect: Rect) -> Interaction {
    let hovered = pointer_over(clip, input, rect);
    Interaction {
        hovered,
        pressed: hovered && input.primary_pressed,
        down: hovered && input.primary_down,
    }
}

/// Whether `rect` has any visible area after the painter's current clip is applied.
pub(super) fn is_visible(painter: &Painter, rect: Rect) -> bool {
    painter.clip_rect().intersect(rect).is_positive()
}

impl Ui<'_> {
    /// Draws a non-interactive line of text. Returns the rectangle it occupied.
    ///
    /// To paint text into an explicit rectangle with both-axis alignment, use [`Ui::text_in`].
    pub fn label(&mut self, text: &str) -> Rect {
        let font = self.ctx.style().font.clone();
        let text_color = self.ctx.style().text_color;
        let spacing = self.spacing();
        let max_width = Some(available_size(&self.layout, spacing).x);
        // Borrow only the fields involved. `layout` keeps `text_cache` borrowed, so everything
        // that runs while it is alive must use disjoint field borrows instead of `&mut self.ctx`.
        let layout = self
            .ctx
            .text_cache
            .layout(&self.ctx.text_system, text, &font, max_width);
        let size = layout.size();
        let rect = allocate_space(&mut self.layout, size, spacing);

        paint_text_rect(
            &mut self.ctx.text_system,
            &mut self.ctx.painter,
            self.ctx.pixels_per_point,
            layout,
            rect,
            Align2::LEFT_TOP,
            text_color,
        );
        rect
    }

    /// Paints text inside `rect` without allocating space or advancing the cursor.
    ///
    /// The text wraps at `rect.width()` and the text block is positioned by `align`. `rect` is in
    /// absolute logical coordinates, like the rectangles returned by widgets. The returned
    /// rectangle is the area occupied by the text block; it may extend outside `rect` when the
    /// text is larger, and painting stays clipped to the enclosing viewport.
    ///
    /// This aligns the whole text block. In wrapped text each line is still laid out from the
    /// left edge of the block.
    pub fn text_in(&mut self, text: &str, rect: Rect, align: Align2, color: Color32) -> Rect {
        let font = self.ctx.style().font.clone();
        let layout =
            self.ctx
                .text_cache
                .layout(&self.ctx.text_system, text, &font, Some(rect.width()));

        paint_text_rect(
            &mut self.ctx.text_system,
            &mut self.ctx.painter,
            self.ctx.pixels_per_point,
            layout,
            rect,
            align,
            color,
        )
    }
}

/// Paints an already laid out text block aligned inside `rect`.
///
/// Returns the rectangle occupied by the text block.
pub(super) fn paint_text_rect(
    text: &mut TextSystem,
    painter: &mut Painter,
    pixels_per_point: f32,
    layout: &TextLayout,
    rect: Rect,
    align: Align2,
    color: Color32,
) -> Rect {
    let size = layout.size();
    let used = Rect::from_min_size(align.anchor(rect, size), size);
    paint_text(text, painter, pixels_per_point, layout, used.min, color);
    used
}

fn paint_text(
    text: &mut TextSystem,
    painter: &mut Painter,
    pixels_per_point: f32,
    layout: &TextLayout,
    origin: Pos2,
    color: Color32,
) {
    if !painter.clip_rect().is_positive() {
        return;
    }
    text.prepare_into(painter, layout, origin, color, pixels_per_point);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_util::context;
    use crate::{Align, RawInput, pos2, vec2};

    #[test]
    fn label_returns_the_allocated_rectangle() {
        let mut context = context();
        let mut label_rect = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            label_rect = ui.label("hello");
        });

        assert!(label_rect.is_positive());
        assert_eq!(label_rect.min, pos2(0.0, 0.0));
    }

    #[test]
    fn align2_anchor_places_content_inside_a_rect() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(100.0, 50.0));
        let size = vec2(20.0, 10.0);

        assert_eq!(Align2::LEFT_TOP.anchor(rect, size), pos2(10.0, 20.0));
        assert_eq!(Align2::CENTER.anchor(rect, size), pos2(50.0, 40.0));
        assert_eq!(Align2::RIGHT_BOTTOM.anchor(rect, size), pos2(90.0, 60.0));
        assert_eq!(Align2::LEFT_CENTER.anchor(rect, size), pos2(10.0, 40.0));
    }

    #[test]
    fn align2_treats_stretch_as_start() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(100.0, 50.0));
        let size = vec2(20.0, 10.0);
        let align = Align2 {
            x: Align::Stretch,
            y: Align::Stretch,
        };

        assert_eq!(align.anchor(rect, size), pos2(10.0, 20.0));
    }

    #[test]
    fn text_in_centers_the_text_block_inside_the_rect() {
        let mut context = context();
        let rect = Rect::from_min_size(pos2(15.0, 25.0), vec2(200.0, 100.0));
        let mut used = Rect::NOTHING;
        let frame = context.run(vec2(300.0, 200.0), 1.0, RawInput::default(), |ui| {
            used = ui.text_in("hello", rect, Align2::CENTER, Color32::WHITE);
        });

        assert!(used.is_positive());
        assert!((used.center().x - rect.center().x).abs() < 0.01);
        assert!((used.center().y - rect.center().y).abs() < 0.01);
        assert!(!frame.paint.vertices.is_empty());
    }

    #[test]
    fn text_in_does_not_advance_the_flow() {
        let mut context = context();
        let mut after = Rect::NOTHING;

        let _ = context.run(vec2(300.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.text_in(
                "hello",
                Rect::from_min_size(pos2(50.0, 50.0), vec2(100.0, 50.0)),
                Align2::CENTER,
                Color32::WHITE,
            );
            after = ui.allocate_space(vec2(10.0, 10.0));
        });

        assert_eq!(after.min, pos2(0.0, 0.0));
    }
}
