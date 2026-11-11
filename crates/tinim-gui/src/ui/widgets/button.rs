//! Buttons and the selectable variants built on top of them.

use tinim_paint::{RectShape, Shape, Vec2};

use super::super::Ui;
use super::super::layout::allocate_space;
use super::super::response::Response;
use super::{interaction, is_visible};
use crate::cursor::CursorIcon;

impl Ui<'_> {
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

    /// Draws a selectable button.
    ///
    /// The widget is selected when `selected` is true. Callers update their own state from
    /// [`Response::pressed`].
    ///
    /// Like [`Ui::button`], this reports a press edge rather than a full click.
    pub fn selectable_label(&mut self, selected: bool, text: &str) -> Response {
        self.draw_button(text, selected)
    }

    fn draw_button(&mut self, text: &str, selected: bool) -> Response {
        let button = self.ctx.style().button.clone();
        let font = self.ctx.style().font.clone();
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
        let rect = allocate_space(&mut self.layout, size, spacing);

        let hit = interaction(self.clip, &self.ctx.input, rect);
        if hit.hovered {
            // The text layout above keeps `text_cache` borrowed; assign the field directly
            // instead of calling `Ui::set_cursor_icon`, which borrows all of `self`.
            self.ctx.platform.cursor_icon = CursorIcon::Pointer;
        }

        if is_visible(&self.ctx.painter, rect) {
            let fill = if selected {
                if hit.down || hit.pressed {
                    button.pressed
                } else {
                    button.selected
                }
            } else if hit.down || hit.pressed {
                button.pressed
            } else if hit.hovered {
                button.hovered
            } else {
                button.normal
            };

            self.ctx.painter.add(Shape::Rect(
                RectShape::new(rect, fill).with_corner_radius(button.corner_radius),
            ));

            let origin = rect.center() - text_size * 0.5;
            let mesh = self.ctx.text_system.prepare(
                layout,
                origin,
                button.text,
                self.ctx.pixels_per_point,
            );
            self.ctx.painter.add_mesh(mesh);
        }

        Response {
            rect,
            hovered: hit.hovered,
            pressed: hit.pressed,
            down: hit.down,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ui::test_util::context;
    use crate::{CursorIcon, RawInput, pos2, vec2};

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
    fn button_requests_pointer_cursor_only_while_hovered() {
        let mut context = context();
        let hover = RawInput {
            pointer: Some(pos2(1.0, 1.0)),
            ..Default::default()
        };

        let frame = context.run(vec2(200.0, 100.0), 1.0, hover, |ui| {
            ui.button("go");
        });
        assert_eq!(frame.platform.cursor_icon, CursorIcon::Pointer);
        drop(frame);

        let frame = context.run(vec2(200.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.button("go");
        });
        assert_eq!(frame.platform.cursor_icon, CursorIcon::Default);
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
    fn button_fill_and_text_share_one_draw() {
        let mut context = context();
        let frame = context.run(vec2(200.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.button("go");
        });

        assert_eq!(
            frame.paint.draws.len(),
            1,
            "the button rectangle and its text use the same atlas"
        );
        assert_eq!(frame.paint.draws[0].texture_id, tinim_paint::TextureId(1));
        assert_eq!(frame.textures.create.len(), 1);
        assert!(
            frame
                .textures
                .set
                .iter()
                .any(|(_, delta)| delta.pos == Some([0, 0]) && delta.size == [1, 1]),
            "the atlas uploads its reserved white texel on creation"
        );
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
}
