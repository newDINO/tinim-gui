//! Rectangular frames with a background, border and padding.

use tinim_paint::{Pos2, Rect, RectShape, Shape, Vec2, vec2};

use super::super::Ui;
use super::super::layout::{Direction, FlowConfig, next_pos};
use crate::style::FrameStyle;

/// The result of building a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameResponse {
    /// The outer rectangle, including the border and padding.
    pub rect: Rect,
    /// The content rectangle after removing the border and padding.
    pub content_rect: Rect,
}

impl Ui<'_> {
    /// Paints a frame background and border over `rect`.
    ///
    /// This is the painting half of [`Ui::sized_frame`] and [`Ui::frame`]. It is public so custom
    /// widgets can share the same background and border rendering without building a container.
    /// It does not change layout or clipping.
    pub fn paint_frame(&mut self, rect: Rect, style: FrameStyle) {
        if style.is_visible() && rect.is_positive() {
            self.ctx.painter.add(Shape::Rect(frame_shape(rect, style)));
        }
    }

    /// Builds content in a fixed-size frame.
    ///
    /// `size` is the outer rectangle, including the border and padding. The frame is allocated in
    /// the parent flow, and its content is laid out vertically in the padded content rectangle.
    /// Like [`Ui::sized_flow`], this does not narrow the clip.
    pub fn sized_frame<R>(
        &mut self,
        size: Vec2,
        style: FrameStyle,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> (R, FrameResponse) {
        let rect = self.allocate_space(size);
        self.paint_frame(rect, style);

        let content_rect = style.content_rect(rect);
        let (result, _used) = self.layout_at(
            content_rect,
            content_rect.min,
            FlowConfig::vertical(),
            build,
        );

        (result, FrameResponse { rect, content_rect })
    }

    /// Builds a frame whose outer size follows its content.
    ///
    /// The content is laid out vertically. Its available area is the current region minus the
    /// frame's border and padding, and the outer rectangle is the content's used rectangle expanded
    /// by the same inset. This is a content-sized flow, so the current region must have a forward
    /// direction; use [`Ui::sized_frame`] inside reverse flows.
    ///
    /// The background and border are painted behind the content. Unlike a [`Ui::pane`], a frame
    /// does not clip its children.
    pub fn frame<R>(
        &mut self,
        style: FrameStyle,
        build: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> (R, FrameResponse) {
        debug_assert!(
            self.layout.direction.is_forward(),
            "content-sized frame requires a forward flow; use sized_frame for reverse directions"
        );

        let start = next_pos(&self.layout, self.spacing());
        let inset = style.inset();
        let content_start = start + vec2(inset.left, inset.top);
        let content_max =
            (self.layout.rect.max - vec2(inset.right, inset.bottom)).max(content_start);
        let content_rect = Rect::from_min_max(content_start, content_max);

        let slot = style
            .is_visible()
            .then(|| reserved_frame_shape(style))
            .and_then(|shape| self.ctx.painter.reserve_rect(shape));

        let (result, used) =
            self.layout_at(content_rect, content_start, FlowConfig::vertical(), build);

        let used = if used.is_positive() {
            used
        } else {
            Rect::from_min_size(content_start, Vec2::ZERO)
        };
        let rect = Rect::from_min_max(
            used.min - vec2(inset.left, inset.top),
            used.max + vec2(inset.right, inset.bottom),
        );

        if let Some(slot) = slot {
            self.ctx.painter.set_rect(slot, frame_shape(rect, style));
        }

        if rect.is_positive() {
            match self.layout.direction {
                Direction::Vertical => {
                    self.layout.cursor.y = self.layout.cursor.y.max(rect.max.y);
                }
                Direction::VerticalReverse => {
                    self.layout.cursor.y = self.layout.cursor.y.min(rect.min.y);
                }
                Direction::Horizontal => {
                    self.layout.cursor.x = self.layout.cursor.x.max(rect.max.x);
                }
                Direction::HorizontalReverse => {
                    self.layout.cursor.x = self.layout.cursor.x.min(rect.min.x);
                }
            }
            self.layout.has_children = true;
            self.layout.used = if self.layout.used.is_positive() {
                self.layout.used.union(rect)
            } else {
                rect
            };
        }

        let content_rect = style.content_rect(rect);
        (result, FrameResponse { rect, content_rect })
    }
}

fn frame_shape(rect: Rect, style: FrameStyle) -> RectShape {
    RectShape::new(rect, style.background)
        .with_corner_radius(style.corner_radius)
        .with_stroke(style.border)
}

fn reserved_frame_shape(style: FrameStyle) -> RectShape {
    // The rectangle only selects the path topology. The final rectangle is supplied by `set_rect`.
    frame_shape(Rect::from_min_size(Pos2::ZERO, vec2(1.0, 1.0)), style)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_util::context;
    use crate::{Color32, Edges, RawInput, Stroke, pos2, vec2};

    fn style() -> FrameStyle {
        FrameStyle {
            background: Color32::from_rgb(30, 30, 34),
            border: Stroke::new(2.0, Color32::from_rgb(90, 90, 98)),
            padding: Edges::all(8.0),
            corner_radius: 0,
        }
    }

    #[test]
    fn sized_frame_insets_content_and_allocates_its_outer_size() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut inner = Rect::NOTHING;
        let mut after = Rect::NOTHING;
        let mut response = None;

        let frame = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            let (_, built) = ui.sized_frame(vec2(100.0, 50.0), style(), |ui| {
                inner = ui.allocate_space(vec2(10.0, 10.0));
            });
            response = Some(built);
            after = ui.allocate_space(vec2(10.0, 10.0));
        });

        let response = response.unwrap();
        assert_eq!(
            response.rect,
            Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 50.0))
        );
        assert_eq!(
            response.content_rect,
            Rect::from_min_size(pos2(10.0, 10.0), vec2(80.0, 30.0))
        );
        assert_eq!(inner.min, pos2(10.0, 10.0));
        assert_eq!(after.min.y, 50.0 + spacing);

        assert!(
            frame
                .paint
                .vertices
                .iter()
                .any(|vertex| vertex.color == style().background.to_array())
        );
        assert!(
            frame
                .paint
                .vertices
                .iter()
                .any(|vertex| vertex.color == style().border.color.to_array())
        );
        assert_eq!(frame.paint.draws.len(), 1);
    }

    #[test]
    fn content_sized_frame_wraps_its_content_and_advances_the_parent() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut content = Rect::NOTHING;
        let mut after = Rect::NOTHING;
        let mut response = None;

        let frame = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            let (_, built) = ui.frame(style(), |ui| {
                content = ui.allocate_space(vec2(20.0, 10.0));
            });
            response = Some(built);
            after = ui.allocate_space(vec2(10.0, 10.0));
        });

        let response = response.unwrap();
        assert_eq!(content.min, pos2(10.0, 10.0));
        assert_eq!(
            response.rect,
            Rect::from_min_size(pos2(0.0, 0.0), vec2(40.0, 30.0))
        );
        assert_eq!(
            response.content_rect,
            Rect::from_min_size(pos2(10.0, 10.0), vec2(20.0, 10.0))
        );
        assert_eq!(after.min.y, response.rect.max.y + spacing);

        assert_eq!(frame.paint.draws.len(), 1);
        assert_eq!(
            frame.paint.vertices[0].pos,
            [0.0, 0.0],
            "the background is painted before the content"
        );
        assert_eq!(frame.paint.vertices[0].color, style().background.to_array());
    }

    #[test]
    fn an_empty_content_sized_frame_still_has_its_inset_size() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut after = Rect::NOTHING;
        let mut response = None;

        let _ = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            let (_, built) = ui.frame(style(), |_| {});
            response = Some(built);
            after = ui.allocate_space(vec2(10.0, 10.0));
        });

        let response = response.unwrap();
        assert_eq!(
            response.rect,
            Rect::from_min_size(pos2(0.0, 0.0), vec2(20.0, 20.0))
        );
        assert!(!response.content_rect.is_positive());
        assert_eq!(after.min.y, 20.0 + spacing);
    }

    #[test]
    fn nested_frames_keep_the_outer_background_behind_the_inner_one() {
        let mut context = context();
        let outer = style();
        let mut inner = style();
        inner.background = Color32::from_rgb(200, 40, 40);
        inner.padding = Edges::all(2.0);

        let frame = context.run(vec2(200.0, 200.0), 1.0, RawInput::default(), |ui| {
            ui.frame(outer, |ui| {
                ui.frame(inner, |ui| {
                    ui.allocate_space(vec2(5.0, 5.0));
                });
            });
        });

        assert_eq!(frame.paint.draws.len(), 1);
        assert_eq!(frame.paint.vertices[0].color, outer.background.to_array());
        assert!(
            frame
                .paint
                .vertices
                .iter()
                .any(|vertex| vertex.color == inner.background.to_array())
        );
    }
}
