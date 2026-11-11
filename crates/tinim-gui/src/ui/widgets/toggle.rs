//! Checkboxes and radio buttons.

use tinim_paint::{CircleShape, Pos2, Rect, RectShape, Shape, Stroke, Vec2};

use super::super::Ui;
use super::super::layout::allocate_space;
use super::super::response::Response;
use super::{interaction, is_visible};
use crate::cursor::CursorIcon;

impl Ui<'_> {
    /// Draws a checkbox without a label and toggles `checked` when it is pressed.
    ///
    /// Like [`Ui::button`], this reports a press edge rather than a full click.
    pub fn checkbox(&mut self, checked: &mut bool) -> Response {
        let checkbox = self.ctx.style().checkbox.clone();
        let spacing = self.spacing();
        let rect = allocate_space(&mut self.layout, checkbox.size, spacing);

        let hit = interaction(self.clip, &self.ctx.input, rect);
        if hit.hovered {
            self.set_cursor_icon(CursorIcon::Pointer);
        }

        if hit.pressed {
            *checked = !*checked;
        }

        if is_visible(&self.ctx.painter, rect) {
            let fill = if hit.down || hit.pressed {
                checkbox.pressed
            } else if *checked {
                checkbox.checked
            } else if hit.hovered {
                checkbox.hovered
            } else {
                checkbox.normal
            };

            self.ctx.painter.add(Shape::Rect(
                RectShape::new(rect, fill)
                    .with_corner_radius(checkbox.corner_radius)
                    .with_stroke(Stroke::new(checkbox.border_width, checkbox.border)),
            ));

            if *checked {
                self.ctx
                    .painter
                    .line(&check_mark(rect), checkbox.check_width, checkbox.check);
            }
        }

        Response {
            rect,
            hovered: hit.hovered,
            pressed: hit.pressed,
            down: hit.down,
        }
    }

    /// Draws a radio button without a label.
    ///
    /// `selected` controls whether the inner dot is drawn. Callers update their own state from
    /// [`Response::pressed`], or use [`Ui::radio_value`] to update a value automatically.
    ///
    /// Like [`Ui::button`], this reports a press edge rather than a full click.
    pub fn radio(&mut self, selected: bool) -> Response {
        let radio = self.ctx.style().radio.clone();
        let spacing = self.spacing();
        let size = Vec2::splat(radio.radius * 2.0);
        let rect = allocate_space(&mut self.layout, size, spacing);

        let hit = interaction(self.clip, &self.ctx.input, rect);
        if hit.hovered {
            self.set_cursor_icon(CursorIcon::Pointer);
        }

        if is_visible(&self.ctx.painter, rect) {
            let fill = if hit.down || hit.pressed {
                radio.pressed
            } else if selected {
                radio.selected
            } else if hit.hovered {
                radio.hovered
            } else {
                radio.normal
            };

            let center = rect.center();
            let radius = rect.width().min(rect.height()) * 0.5;
            self.ctx.painter.add(Shape::Circle(
                CircleShape::new(center, radius, fill)
                    .with_stroke(Stroke::new(radio.border_width, radio.border)),
            ));

            if selected {
                let dot_radius = radio.dot_radius.min(radius);
                self.ctx.painter.circle(center, dot_radius, radio.dot);
            }
        }

        Response {
            rect,
            hovered: hit.hovered,
            pressed: hit.pressed,
            down: hit.down,
        }
    }

    /// Draws a radio button without a label and updates `current_value` when it is pressed.
    ///
    /// The radio is selected when `current_value == selected_value`.
    ///
    /// Like [`Ui::button`], this reports a press edge rather than a full click.
    pub fn radio_value<Value: PartialEq>(
        &mut self,
        current_value: &mut Value,
        selected_value: Value,
    ) -> Response {
        let selected = *current_value == selected_value;
        let response = self.radio(selected);
        if response.pressed {
            *current_value = selected_value;
        }
        response
    }
}

/// Check-mark polyline inside `rect`, from the lower-left to the upper-right.
fn check_mark(rect: Rect) -> [Pos2; 3] {
    let size = rect.size();
    [
        Pos2::new(rect.min.x + size.x * 0.24, rect.min.y + size.y * 0.52),
        Pos2::new(rect.min.x + size.x * 0.42, rect.min.y + size.y * 0.70),
        Pos2::new(rect.min.x + size.x * 0.78, rect.min.y + size.y * 0.30),
    ]
}

#[cfg(test)]
mod tests {
    use crate::ui::test_util::context;
    use crate::{Color32, CursorIcon, RawInput, pos2, vec2};

    #[test]
    fn checkbox_requests_pointer_cursor_while_hovered() {
        let mut context = context();
        let mut checked = false;
        let hover = RawInput {
            pointer: Some(pos2(1.0, 1.0)),
            ..Default::default()
        };

        let frame = context.run(vec2(200.0, 100.0), 1.0, hover, |ui| {
            ui.checkbox(&mut checked);
        });
        assert_eq!(frame.platform.cursor_icon, CursorIcon::Pointer);
    }

    #[test]
    fn checkbox_toggles_when_pressed() {
        let mut context = context();
        let mut checked = false;

        let input = RawInput {
            pointer: Some(pos2(1.0, 1.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(200.0, 100.0), 1.0, input, |ui| {
            ui.checkbox(&mut checked);
        });

        assert!(checked);
    }

    #[test]
    fn checkbox_keeps_the_value_when_not_pressed() {
        let mut context = context();
        let mut checked = true;

        let _ = context.run(vec2(200.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.checkbox(&mut checked);
        });

        assert!(checked);
    }

    #[test]
    fn checkbox_does_not_toggle_when_pointer_is_outside() {
        let mut context = context();
        let mut checked = false;

        let input = RawInput {
            pointer: Some(pos2(500.0, 500.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(200.0, 100.0), 1.0, input, |ui| {
            ui.checkbox(&mut checked);
        });

        assert!(!checked);
    }

    #[test]
    fn checkbox_paints_a_check_mark_only_when_checked() {
        let mut context = context();
        let check = Color32::from_rgb(1, 2, 3);
        context.style_mut().checkbox.check = check;

        let has_check = |context: &mut crate::Context, checked: &mut bool| {
            let frame = context.run(vec2(200.0, 100.0), 1.0, RawInput::default(), |ui| {
                ui.checkbox(checked);
            });
            frame
                .paint
                .vertices
                .iter()
                .any(|vertex| vertex.color == check.to_array())
        };

        assert!(!has_check(&mut context, &mut false));
        assert!(has_check(&mut context, &mut true));
    }

    #[test]
    fn radio_reports_press_when_pointer_is_over_it() {
        let mut context = context();
        let mut pressed = None;

        let input = RawInput {
            pointer: Some(pos2(1.0, 1.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(200.0, 100.0), 1.0, input, |ui| {
            pressed = Some(ui.radio(false).pressed);
        });

        assert_eq!(pressed, Some(true));
    }

    #[test]
    fn radio_value_updates_the_value_when_pressed() {
        let mut context = context();
        let mut value = 1;

        let input = RawInput {
            pointer: Some(pos2(1.0, 1.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(200.0, 100.0), 1.0, input, |ui| {
            ui.radio_value(&mut value, 2);
        });

        assert_eq!(value, 2);
    }

    #[test]
    fn radio_value_keeps_the_value_when_not_pressed() {
        let mut context = context();
        let mut value = 1;

        let _ = context.run(vec2(200.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.radio_value(&mut value, 2);
        });

        assert_eq!(value, 1);
    }

    #[test]
    fn radio_paints_a_dot_only_when_selected() {
        let mut context = context();
        let dot = Color32::from_rgb(4, 5, 6);
        context.style_mut().radio.dot = dot;

        let has_dot = |context: &mut crate::Context, selected: bool| {
            let frame = context.run(vec2(200.0, 100.0), 1.0, RawInput::default(), |ui| {
                ui.radio(selected);
            });
            frame
                .paint
                .vertices
                .iter()
                .any(|vertex| vertex.color == dot.to_array())
        };

        assert!(!has_dot(&mut context, false));
        assert!(has_dot(&mut context, true));
    }
}
