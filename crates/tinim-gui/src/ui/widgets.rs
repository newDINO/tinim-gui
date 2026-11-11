//! Leaf widgets: text labels, buttons, checkboxes, radios, sliders and number inputs.

use std::ops::RangeInclusive;

use tinim_paint::{CircleShape, Color32, Painter, Pos2, Rect, RectShape, Shape, Stroke, Vec2};
use tinim_paint_text::TextSystem;
use tinim_text::TextLayout;

use super::Ui;
use super::layout::{allocate_space, available_size, pointer_over};
use super::response::{NumberInputResponse, NumberInputState, Response};
use crate::cursor::CursorIcon;

impl Ui<'_> {
    /// Draws a non-interactive line of text. Returns the rectangle it occupied.
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

        paint_text(
            &mut self.ctx.text_system,
            &mut self.ctx.painter,
            self.ctx.pixels_per_point,
            layout,
            rect.min,
            text_color,
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

    /// Draws a selectable button.
    ///
    /// The widget is selected when `selected` is true. Callers update their own state from
    /// [`Response::pressed`].
    ///
    /// Like [`Ui::button`], this reports a press edge rather than a full click.
    pub fn selectable_label(&mut self, selected: bool, text: &str) -> Response {
        self.draw_button(text, selected)
    }

    /// Draws a checkbox without a label and toggles `checked` when it is pressed.
    ///
    /// Like [`Ui::button`], this reports a press edge rather than a full click.
    pub fn checkbox(&mut self, checked: &mut bool) -> Response {
        let checkbox = self.ctx.style().checkbox.clone();
        let spacing = self.spacing();
        let rect = allocate_space(&mut self.layout, checkbox.size, spacing);

        let hovered = pointer_over(self.clip, &self.ctx.input, rect);
        let pressed = hovered && self.ctx.input.primary_pressed;
        let down = hovered && self.ctx.input.primary_down;
        if hovered {
            self.set_cursor_icon(CursorIcon::Pointer);
        }

        if pressed {
            *checked = !*checked;
        }

        let clip = self.ctx.painter.clip_rect();
        if clip.intersect(rect).is_positive() {
            let fill = if down || pressed {
                checkbox.pressed
            } else if *checked {
                checkbox.checked
            } else if hovered {
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
            hovered,
            pressed,
            down,
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

        let hovered = pointer_over(self.clip, &self.ctx.input, rect);
        let pressed = hovered && self.ctx.input.primary_pressed;
        let down = hovered && self.ctx.input.primary_down;
        if hovered {
            self.set_cursor_icon(CursorIcon::Pointer);
        }

        let clip = self.ctx.painter.clip_rect();
        if clip.intersect(rect).is_positive() {
            let fill = if down || pressed {
                radio.pressed
            } else if selected {
                radio.selected
            } else if hovered {
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
            hovered,
            pressed,
            down,
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

    /// Draws a horizontal slider and updates `value` while the track is pressed or dragged.
    ///
    /// The range is inclusive. Values outside the range are clamped, and a reversed range such as
    /// `1.0..=0.0` is treated as `0.0..=1.0`. Non-finite bounds make the slider read-only.
    ///
    /// Like [`Ui::button`], this reports a press edge rather than a full click. Dragging only
    /// continues while the pointer stays over the slider rectangle.
    pub fn slider(&mut self, value: &mut f32, range: RangeInclusive<f32>) -> Response {
        let slider = self.ctx.style().slider.clone();
        let track_height = slider.track_height.max(0.0);
        let handle_radius = slider.handle_radius.max(0.0);
        let spacing = self.spacing();
        let available = available_size(&self.layout, spacing);
        let size = Vec2::new(
            available.x.min(slider.min_size.x).max(0.0),
            slider.min_size.y.max(track_height).max(handle_radius * 2.0),
        );
        let rect = allocate_space(&mut self.layout, size, spacing);

        let (min, max) = ordered_range(&range);
        let interactive = min.is_finite() && max.is_finite() && max > min;
        if interactive {
            *value = clamp_slider_value(*value, min, max);
        }

        let hovered = pointer_over(self.clip, &self.ctx.input, rect);
        let pressed = hovered && self.ctx.input.primary_pressed;
        let down = hovered && self.ctx.input.primary_down;
        if hovered {
            self.set_cursor_icon(CursorIcon::Pointer);
        }

        // Inset the track by the handle radius so the handle stays inside the widget at either end.
        let padding = handle_radius.min(rect.width() * 0.5);
        let center_y = rect.center().y;
        let track = Rect::from_min_max(
            Pos2::new(rect.min.x + padding, center_y - track_height * 0.5),
            Pos2::new(rect.max.x - padding, center_y + track_height * 0.5),
        );
        let track_width = track.width();

        if interactive
            && track_width > 0.0
            && (pressed || down)
            && let Some(pointer) = self.ctx.input.pointer
        {
            let t = ((pointer.x - track.min.x) / track_width).clamp(0.0, 1.0);
            *value = min + (max - min) * t;
        }

        let t = if interactive && track_width > 0.0 {
            ((*value - min) / (max - min)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let handle_x = track.min.x + track_width * t;

        let clip = self.ctx.painter.clip_rect();
        if clip.intersect(rect).is_positive() {
            let corner_radius = (track_height * 0.5).round().clamp(0.0, 255.0) as u8;
            self.ctx.painter.add(Shape::Rect(
                RectShape::new(track, slider.track).with_corner_radius(corner_radius),
            ));

            if handle_x > track.min.x {
                let filled = Rect::from_min_max(track.min, Pos2::new(handle_x, track.max.y));
                self.ctx.painter.add(Shape::Rect(
                    RectShape::new(filled, slider.filled).with_corner_radius(corner_radius),
                ));
            }

            let handle_color = if down || pressed {
                slider.handle_active
            } else if hovered {
                slider.handle_hovered
            } else {
                slider.handle
            };
            self.ctx.painter.add(Shape::Circle(CircleShape::new(
                Pos2::new(handle_x, center_y),
                handle_radius,
                handle_color,
            )));
        }

        Response {
            rect,
            hovered,
            pressed,
            down,
        }
    }

    /// Draws a numeric text input and updates `value` when the text parses as a finite number.
    ///
    /// The field uses [`TextEditStyle`](crate::TextEditStyle). Clicking focuses it, Enter commits
    /// the value, and clicking elsewhere commits a valid number or restores the text from `value`.
    /// While the input is focused, external changes to `value` do not overwrite the text. The text
    /// wins when the input is committed, so an external change made during editing is discarded.
    ///
    /// `state` is caller-owned, like [`TextEditState`](crate::TextEditState) for
    /// [`Ui::text_edit_singleline`].
    pub fn number_input(
        &mut self,
        value: &mut f32,
        state: &mut NumberInputState,
    ) -> NumberInputResponse {
        let was_focused = state.edit.is_focused();
        if !was_focused {
            let text = format_number(*value);
            if state.text != text {
                state.text = text;
            }
        }

        let response = self.text_edit_singleline(&mut state.text, &mut state.edit);
        let mut changed = false;
        let mut submitted = false;

        if response.changed
            && let Some(parsed) = parse_number(&state.text)
            && parsed != *value
        {
            *value = parsed;
            changed = true;
        }

        if response.submitted {
            submitted = true;
            if let Some(parsed) = parse_number(&state.text)
                && parsed != *value
            {
                *value = parsed;
                changed = true;
            }
            state.text = format_number(*value);
        }

        // A click outside blurs the editor. Commit a valid number, otherwise discard the edit.
        if was_focused && !response.focused {
            if let Some(parsed) = parse_number(&state.text)
                && parsed != *value
            {
                *value = parsed;
                changed = true;
            }
            state.text = format_number(*value);
        }

        NumberInputResponse {
            rect: response.rect,
            hovered: response.hovered,
            focused: response.focused,
            changed,
            submitted,
        }
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

        let hovered = pointer_over(self.clip, &self.ctx.input, rect);
        let pressed = hovered && self.ctx.input.primary_pressed;
        let down = hovered && self.ctx.input.primary_down;
        if hovered {
            // The text layout above keeps `text_cache` borrowed; assign the field directly
            // instead of calling `Ui::set_cursor_icon`, which borrows all of `self`.
            self.ctx.platform.cursor_icon = CursorIcon::Pointer;
        }

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
            hovered,
            pressed,
            down,
        }
    }
}

/// Returns the bounds in ascending order. Non-finite or reversed inputs are left for the caller
/// to reject.
fn ordered_range(range: &RangeInclusive<f32>) -> (f32, f32) {
    let start = *range.start();
    let end = *range.end();
    if start <= end {
        (start, end)
    } else {
        (end, start)
    }
}

fn clamp_slider_value(value: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        min
    }
}

fn format_number(value: f32) -> String {
    if value == 0.0 {
        // Avoid showing "-0" after the user drags the slider to the bottom of a range.
        "0".to_owned()
    } else {
        value.to_string()
    }
}

fn parse_number(text: &str) -> Option<f32> {
    text.trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
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
    use crate::{CursorIcon, Key, RawInput, pos2, vec2};

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
    fn slider_maps_the_pointer_position_to_the_value() {
        let mut context = context();
        let mut value = 0.0;
        let mut rect = Rect::NOTHING;

        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            rect = ui.slider(&mut value, 0.0..=10.0).rect;
        });

        let input = RawInput {
            pointer: Some(rect.center()),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, input, |ui| {
            ui.slider(&mut value, 0.0..=10.0);
        });

        assert!((value - 5.0).abs() < 0.01, "value was {value}");
    }

    #[test]
    fn slider_clamps_values_to_the_range() {
        let mut context = context();
        let mut value = 20.0;

        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.slider(&mut value, 0.0..=10.0);
        });

        assert_eq!(value, 10.0);
    }

    #[test]
    fn slider_accepts_a_reversed_range() {
        let mut context = context();
        let mut value = 5.0;
        let mut rect = Rect::NOTHING;

        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            rect = ui.slider(&mut value, 10.0..=0.0).rect;
        });

        let input = RawInput {
            pointer: Some(rect.center()),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, input, |ui| {
            ui.slider(&mut value, 10.0..=0.0);
        });

        assert!((value - 5.0).abs() < 0.01, "value was {value}");
    }

    #[test]
    fn slider_keeps_the_value_while_hovered_without_a_press() {
        let mut context = context();
        let mut value = 3.0;
        let mut rect = Rect::NOTHING;

        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            rect = ui.slider(&mut value, 0.0..=10.0).rect;
        });

        let hover = RawInput {
            pointer: Some(rect.center()),
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, hover, |ui| {
            ui.slider(&mut value, 0.0..=10.0);
        });

        assert_eq!(value, 3.0);
    }

    #[test]
    fn slider_paints_the_filled_track_and_handle() {
        let mut context = context();
        let filled = context.style().slider.filled;
        let handle = context.style().slider.handle;
        let mut value = 0.5;

        let frame = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.slider(&mut value, 0.0..=1.0);
        });

        assert!(
            frame
                .paint
                .vertices
                .iter()
                .any(|vertex| vertex.color == filled.to_array())
        );
        assert!(
            frame
                .paint
                .vertices
                .iter()
                .any(|vertex| vertex.color == handle.to_array())
        );
    }

    #[test]
    fn slider_requests_the_pointer_cursor_while_hovered() {
        let mut context = context();
        let mut value = 0.0;
        let mut rect = Rect::NOTHING;

        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            rect = ui.slider(&mut value, 0.0..=1.0).rect;
        });

        let hover = RawInput {
            pointer: Some(rect.center()),
            ..Default::default()
        };
        let frame = context.run(vec2(300.0, 100.0), 1.0, hover, |ui| {
            ui.slider(&mut value, 0.0..=1.0);
        });

        assert_eq!(frame.platform.cursor_icon, CursorIcon::Pointer);
    }

    #[test]
    fn number_input_commits_a_typed_value() {
        let mut context = context();
        let mut value = 0.0;
        let mut state = NumberInputState::new();
        let mut rect = Rect::NOTHING;

        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            rect = ui.number_input(&mut value, &mut state).rect;
        });

        let press = RawInput {
            pointer: Some(rect.min + vec2(1.0, 1.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, press, |ui| {
            ui.number_input(&mut value, &mut state);
        });
        assert!(state.is_focused());

        let typed = RawInput {
            text: "12.5".to_owned(),
            ..Default::default()
        };
        let mut changed = false;
        let _ = context.run(vec2(300.0, 100.0), 1.0, typed, |ui| {
            changed = ui.number_input(&mut value, &mut state).changed;
        });

        assert!(changed);
        assert_eq!(value, 12.5);
    }

    #[test]
    fn number_input_discards_invalid_text_on_blur() {
        let mut context = context();
        let mut value = 7.0;
        let mut state = NumberInputState::new();
        let mut rect = Rect::NOTHING;

        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            rect = ui.number_input(&mut value, &mut state).rect;
        });
        let press = RawInput {
            pointer: Some(rect.min + vec2(1.0, 1.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, press, |ui| {
            ui.number_input(&mut value, &mut state);
        });
        let typed = RawInput {
            text: "not a number".to_owned(),
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, typed, |ui| {
            ui.number_input(&mut value, &mut state);
        });
        assert_eq!(value, 7.0);

        let outside = RawInput {
            pointer: Some(pos2(500.0, 500.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, outside, |ui| {
            ui.number_input(&mut value, &mut state);
        });

        assert_eq!(value, 7.0);
        assert_eq!(state.text(), "7");
    }

    #[test]
    fn number_input_tracks_external_value_changes_while_blurred() {
        let mut context = context();
        let mut value = 1.0;
        let mut state = NumberInputState::new();

        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.number_input(&mut value, &mut state);
        });
        assert_eq!(state.text(), "1");

        value = 2.0;
        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.number_input(&mut value, &mut state);
        });
        assert_eq!(state.text(), "2");
    }

    #[test]
    fn number_input_keeps_external_changes_out_while_focused() {
        let mut context = context();
        let mut value = 1.0;
        let mut state = NumberInputState::new();
        let mut rect = Rect::NOTHING;

        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            rect = ui.number_input(&mut value, &mut state).rect;
        });
        let press = RawInput {
            pointer: Some(rect.min + vec2(1.0, 1.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, press, |ui| {
            ui.number_input(&mut value, &mut state);
        });

        value = 9.0;
        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.number_input(&mut value, &mut state);
        });
        assert_eq!(state.text(), "1");

        // Moving to the end and typing appends to the text that was already being edited.
        let end = RawInput {
            keys: vec![Key::End],
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, end, |ui| {
            ui.number_input(&mut value, &mut state);
        });
        let typed = RawInput {
            text: "2".to_owned(),
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, typed, |ui| {
            ui.number_input(&mut value, &mut state);
        });
        assert_eq!(value, 12.0);
    }

    #[test]
    fn number_input_enter_normalizes_the_text() {
        let mut context = context();
        let mut value = 1.5;
        let mut state = NumberInputState::new();
        let mut rect = Rect::NOTHING;

        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            rect = ui.number_input(&mut value, &mut state).rect;
        });
        let press = RawInput {
            pointer: Some(rect.min + vec2(1.0, 1.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, press, |ui| {
            ui.number_input(&mut value, &mut state);
        });
        let typed = RawInput {
            text: "0".to_owned(),
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, typed, |ui| {
            ui.number_input(&mut value, &mut state);
        });
        assert_eq!(state.text(), "01.5");

        let enter = RawInput {
            keys: vec![Key::Enter],
            ..Default::default()
        };
        let mut submitted = false;
        let _ = context.run(vec2(300.0, 100.0), 1.0, enter, |ui| {
            submitted = ui.number_input(&mut value, &mut state).submitted;
        });

        assert!(submitted);
        assert_eq!(value, 1.5);
        assert_eq!(state.text(), "1.5");
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
