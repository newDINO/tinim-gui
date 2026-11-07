//! Leaf widgets: text labels and buttons.

use tinim_paint::{Color32, Painter, Pos2, Rect, RectShape, Shape, Vec2};
use tinim_text::{TextLayout, TextSystem};

use super::Ui;
use super::layout::{allocate_space, available_size, pointer_over};
use super::response::Response;

impl Ui<'_> {
    /// Draws a non-interactive line of text. Returns the rectangle it occupied.
    pub fn label(&mut self, text: &str) -> Rect {
        let font = self.ctx.style().font.clone();
        let spacing = self.spacing();
        let max_width = Some(available_size(&self.region, spacing).x);
        // Borrow only the fields involved. `layout` keeps `text_cache` borrowed, so everything
        // that runs while it is alive must use disjoint field borrows instead of `&mut self.ctx`.
        let layout = self
            .ctx
            .text_cache
            .layout(&self.ctx.text_system, text, &font, max_width);
        let size = layout.size();
        let rect = allocate_space(&mut self.region, size, spacing);

        paint_text(
            &mut self.ctx.text_system,
            &mut self.ctx.painter,
            self.ctx.pixels_per_point,
            layout,
            rect.min,
            font.color,
        );
        rect
    }

    /// Draws a button.
    ///
    /// v0.1 only reports a press edge; there is no click state, id or press/release pairing.
    pub fn button(&mut self, text: &str) -> Response {
        self.draw_button(text, false)
    }

    /// Draws a selectable button and updates `current_value` when it is pressed.
    ///
    /// The widget is selected when `current_value == selected_value`.
    ///
    /// Like [`Ui::button`], this reports a press edge rather than a full click.
    pub fn selectable_value<Value: PartialEq>(
        &mut self,
        current_value: &mut Value,
        selected_value: Value,
        text: &str,
    ) -> Response {
        let selected = *current_value == selected_value;
        let response = self.draw_button(text, selected);
        if response.pressed {
            *current_value = selected_value;
        }
        response
    }

    pub fn selectable_label(&mut self, selected: bool, text: &str) -> Response {
        self.draw_button(text, selected)
    }

    fn draw_button(&mut self, text: &str, selected: bool) -> Response {
        let button = self.ctx.style().button.clone();
        let mut font = self.ctx.style().font.clone();
        font.color = button.text;
        let spacing = self.spacing();

        let layout = self
            .ctx
            .text_cache
            .layout(&self.ctx.text_system, text, &font, None);
        let text_size = layout.size();
        let size = Vec2::new(
            (text_size.x + button.padding.x * 2.0).max(button.min_size.x),
            (text_size.y + button.padding.y * 2.0).max(button.min_size.y),
        );
        let rect = allocate_space(&mut self.region, size, spacing);

        let hovered = pointer_over(&self.region, &self.ctx.input, rect);
        let pressed = hovered && self.ctx.input.primary_pressed;
        let down = hovered && self.ctx.input.primary_down;

        let clip = self.ctx.painter.clip_rect();
        if clip.intersect(rect).is_positive() {
            let fill = if selected {
                if down || pressed {
                    button.pressed
                } else {
                    button.selected
                }
            } else if down || pressed {
                button.pressed
            } else if hovered {
                button.hovered
            } else {
                button.normal
            };

            self.ctx.painter.add(Shape::Rect(
                RectShape::new(rect, fill).with_corner_radius(button.corner_radius),
            ));

            let origin = rect.center() - text_size * 0.5;
            let mesh =
                self.ctx
                    .text_system
                    .prepare(layout, origin, font.color, self.ctx.pixels_per_point);
            self.ctx.painter.add_mesh(mesh);
        }

        Response {
            rect,
            hovered,
            pressed,
            down,
        }
    }
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
    let mesh = text.prepare(layout, origin, color, pixels_per_point);
    painter.add_mesh(mesh);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_util::context;
    use crate::{RawInput, pos2, vec2};

    #[test]
    fn button_reports_press_only_when_pointer_is_over_it() {
        let mut context = context();
        let mut pressed = None;

        let input = RawInput {
            pointer: Some(pos2(1.0, 1.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(200.0, 100.0), 1.0, input, |ui| {
            pressed = Some(ui.button("go").pressed);
        });

        assert_eq!(pressed, Some(true));
    }

    #[test]
    fn button_does_not_press_when_pointer_is_outside() {
        let mut context = context();
        let mut pressed = None;

        let input = RawInput {
            pointer: Some(pos2(500.0, 500.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(200.0, 100.0), 1.0, input, |ui| {
            pressed = Some(ui.button("go").pressed);
        });

        assert_eq!(pressed, Some(false));
    }

    #[test]
    fn selectable_value_updates_the_value_when_pressed() {
        let mut context = context();
        let mut value = 1;

        let input = RawInput {
            pointer: Some(pos2(1.0, 1.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(200.0, 100.0), 1.0, input, |ui| {
            ui.selectable_value(&mut value, 2, "two");
        });

        assert_eq!(value, 2);
    }

    #[test]
    fn selectable_value_keeps_the_value_when_not_pressed() {
        let mut context = context();
        let mut value = 1;

        let _ = context.run(vec2(200.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.selectable_value(&mut value, 2, "two");
        });

        assert_eq!(value, 1);
    }

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
}
