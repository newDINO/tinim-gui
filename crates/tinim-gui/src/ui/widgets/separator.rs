//! Non-interactive separator lines.

use tinim_paint::{Rect, Vec2};

use super::super::Ui;
use super::super::layout::{Axis, allocate_space, available_size};
use super::is_visible;

impl Ui<'_> {
    /// Draws a separator perpendicular to the current flow direction.
    ///
    /// In a vertical flow this is a horizontal line spanning the region's width; in a horizontal
    /// flow it is a vertical line spanning the region's height. Returns the rectangle it occupied.
    ///
    /// The line color and thickness come from [`SeparatorStyle`](crate::SeparatorStyle). Like
    /// other widgets, the separator is separated from its neighbours by
    /// [`Style::spacing`](crate::Style::spacing).
    pub fn separator(&mut self) -> Rect {
        let separator = self.ctx.style().separator.clone();
        let thickness = separator.thickness.max(0.0);
        let spacing = self.spacing();
        let available = available_size(&self.layout, spacing);
        let size = match self.layout.direction.axis() {
            Axis::Vertical => Vec2::new(available.x, thickness),
            Axis::Horizontal => Vec2::new(thickness, available.y),
        };
        let rect = allocate_space(&mut self.layout, size, spacing);

        if is_visible(&self.ctx.painter, rect) {
            self.ctx.painter.rect(rect, separator.color);
        }

        rect
    }
}

#[cfg(test)]
mod tests {
    use crate::ui::test_util::context;
    use crate::{Align, Color32, FlowConfig, RawInput, Rect, vec2};

    #[test]
    fn separator_is_horizontal_in_a_vertical_flow() {
        let mut context = context();
        let thickness = context.style().separator.thickness;
        let mut rect = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 100.0), 1.0, RawInput::default(), |ui| {
            rect = ui.separator();
        });

        assert_eq!(rect.size(), vec2(200.0, thickness));
    }

    #[test]
    fn separator_is_vertical_in_a_horizontal_flow() {
        let mut context = context();
        let thickness = context.style().separator.thickness;
        let mut rect = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.sized_flow(vec2(120.0, 40.0), FlowConfig::horizontal(), |ui| {
                rect = ui.separator();
            });
        });

        assert_eq!(rect.size(), vec2(thickness, 40.0));
    }

    #[test]
    fn separator_spans_the_cross_axis_regardless_of_alignment() {
        let mut context = context();
        let mut rect = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.sized_flow(
                vec2(120.0, 40.0),
                FlowConfig::vertical().align(Align::Center),
                |ui| {
                    rect = ui.separator();
                },
            );
        });

        assert_eq!(rect.width(), 120.0);
    }

    #[test]
    fn separator_uses_the_configured_color() {
        let mut context = context();
        let color = Color32::from_rgb(1, 2, 3);
        context.style_mut().separator.color = color;

        let frame = context.run(vec2(200.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.separator();
        });

        assert!(
            frame
                .paint
                .vertices
                .iter()
                .any(|vertex| vertex.color == color.to_array())
        );
    }

    #[test]
    fn consecutive_separators_use_the_style_spacing() {
        let mut context = context();
        let spacing = context.style().spacing;
        let mut first = Rect::NOTHING;
        let mut second = Rect::NOTHING;

        let _ = context.run(vec2(200.0, 100.0), 1.0, RawInput::default(), |ui| {
            first = ui.separator();
            second = ui.separator();
        });

        assert_eq!(second.min.y, first.max.y + spacing);
    }
}
