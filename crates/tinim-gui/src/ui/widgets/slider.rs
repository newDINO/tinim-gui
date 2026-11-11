//! Horizontal sliders.

use std::ops::RangeInclusive;

use tinim_paint::{CircleShape, Pos2, Rect, RectShape, Shape, Vec2};

use super::super::Ui;
use super::super::layout::{allocate_space, available_size};
use super::super::response::Response;
use super::{interaction, is_visible};
use crate::cursor::CursorIcon;

impl Ui<'_> {
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

        let hit = interaction(self.clip, &self.ctx.input, rect);
        if hit.hovered {
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
            && (hit.pressed || hit.down)
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

        if is_visible(&self.ctx.painter, rect) {
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

            let handle_color = if hit.down || hit.pressed {
                slider.handle_active
            } else if hit.hovered {
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
            hovered: hit.hovered,
            pressed: hit.pressed,
            down: hit.down,
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

#[cfg(test)]
mod tests {
    use crate::ui::test_util::context;
    use crate::{CursorIcon, RawInput, Rect, vec2};

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
}
