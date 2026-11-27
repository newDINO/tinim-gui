//! Compact numeric values that change when dragged and become a text field when clicked.

use std::ops::RangeInclusive;

use tinim_paint::{Pos2, Rect, RectShape, Shape, Stroke, Vec2};

use super::super::Ui;
use super::super::layout::{Align2, allocate_space, available_size};
use super::number_input::NumberInputState;
use super::{interaction, is_visible, paint_text_rect};
use crate::cursor::CursorIcon;

/// How many logical points the pointer must move before a press becomes a drag.
///
/// Below this distance the press is treated as a click, which switches the widget to text editing.
const DRAG_THRESHOLD: f32 = 2.0;

/// Decimal places are capped so a hostile or mistaken option cannot allocate an enormous string.
const MAX_DECIMALS: usize = 12;

/// Caller-owned state for [`Ui::drag_value`](crate::Ui::drag_value) and
/// [`Ui::drag_value_with`](crate::Ui::drag_value_with).
///
/// The state remembers the value and pointer at the start of a drag, plus the text editor used
/// after a click. Keep one value per widget.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DragValueState {
    edit: NumberInputState,
    drag: Option<DragStart>,
}

/// A drag in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
struct DragStart {
    /// Pointer position seen on the previous frame.
    pointer: Pos2,
    /// Unrounded value accumulated by the drag.
    value: f32,
    /// Movement accumulated before the drag threshold was crossed.
    accumulated: f32,
    /// Whether the pointer moved far enough to count as a drag rather than a click.
    moved: bool,
}

impl DragValueState {
    /// An idle widget that is neither dragged nor being edited.
    pub const fn new() -> Self {
        Self {
            edit: NumberInputState::new(),
            drag: None,
        }
    }

    /// Whether the widget currently shows its text editor and owns keyboard input.
    pub fn is_focused(&self) -> bool {
        self.edit.is_focused()
    }

    /// Focuses or blurs the text editor.
    ///
    /// Blurring also cancels any drag in progress. Focusing directly does not fill the editor;
    /// clicking the widget does that.
    pub fn set_focused(&mut self, focused: bool) {
        self.edit.set_focused(focused);
        if !focused {
            self.drag = None;
        }
    }

    /// Whether the pointer is currently dragging the value.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some_and(|drag| drag.moved)
    }
}

/// Options for [`Ui::drag_value_with`](crate::Ui::drag_value_with).
#[derive(Clone, Debug, PartialEq)]
pub struct DragValueOptions {
    /// Value change per logical point of pointer movement.
    ///
    /// Defaults to `1.0`. A non-finite or non-positive speed disables dragging.
    pub speed: f32,
    /// Optional inclusive range. Values outside it are clamped.
    pub range: Option<RangeInclusive<f32>>,
    /// Whether a value outside [`DragValueOptions::range`] is clamped even before the user
    /// interacts with the widget.
    ///
    /// Defaults to `true`, matching the behavior of a value entering a range for the first time.
    pub clamp_existing_to_range: bool,
    /// Text shown before the number.
    pub prefix: String,
    /// Text shown after the number.
    pub suffix: String,
    /// Minimum number of decimal places shown. Defaults to `0`.
    pub min_decimals: usize,
    /// Maximum number of decimal places shown and rounded to.
    ///
    /// `None` uses the shortest representation of the value. Defaults to `Some(6)`, which keeps
    /// values produced by dragging from showing long floating-point artifacts.
    pub max_decimals: Option<usize>,
}

impl Default for DragValueOptions {
    fn default() -> Self {
        Self {
            speed: 1.0,
            range: None,
            clamp_existing_to_range: true,
            prefix: String::new(),
            suffix: String::new(),
            min_decimals: 0,
            max_decimals: Some(6),
        }
    }
}

impl DragValueOptions {
    /// Sets the value change per logical point of pointer movement.
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    /// Sets an inclusive range and clamps values to it.
    pub fn with_range(mut self, range: RangeInclusive<f32>) -> Self {
        self.range = Some(range);
        self
    }

    /// Keeps values outside [`DragValueOptions::range`] untouched until the user edits them.
    pub fn clamp_existing_to_range(mut self, clamp_existing_to_range: bool) -> Self {
        self.clamp_existing_to_range = clamp_existing_to_range;
        self
    }

    /// Shows `prefix` before the number.
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = prefix.into();
        self
    }

    /// Shows `suffix` after the number.
    pub fn with_suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = suffix.into();
        self
    }

    /// Keeps at least `min_decimals` decimal places.
    pub fn with_min_decimals(mut self, min_decimals: usize) -> Self {
        self.min_decimals = min_decimals;
        self
    }

    /// Rounds to at most `max_decimals` decimal places.
    pub fn with_max_decimals(mut self, max_decimals: usize) -> Self {
        self.max_decimals = Some(max_decimals);
        self
    }

    /// Chooses the maximum number of decimal places, or uses the shortest representation with
    /// `None`.
    pub fn with_max_decimals_opt(mut self, max_decimals: Option<usize>) -> Self {
        self.max_decimals = max_decimals;
        self
    }

    /// Shows exactly `decimals` decimal places.
    pub fn with_fixed_decimals(mut self, decimals: usize) -> Self {
        self.min_decimals = decimals;
        self.max_decimals = Some(decimals);
        self
    }
}

/// The result of building a drag value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragValueResponse {
    pub rect: Rect,
    pub hovered: bool,
    /// The widget is in text-edit mode and owns keyboard input.
    pub focused: bool,
    /// The numeric value changed during this frame.
    pub changed: bool,
    /// The pointer is currently dragging the value.
    pub dragged: bool,
    /// Enter was pressed while editing the text.
    pub submitted: bool,
}

impl Ui<'_> {
    /// Draws a compact numeric value that changes by dragging and becomes editable when clicked.
    ///
    /// Dragging right or up increases the value and dragging left or down decreases it. Holding
    /// Shift divides [`DragValueOptions::speed`] by ten for fine control. A press that does not
    /// move past the drag threshold is a click: the next frame shows a text field with the number
    /// selected, and Enter or clicking elsewhere commits it.
    ///
    /// `state` is caller-owned. Use [`DragValueOptions`] through
    /// [`Ui::drag_value_with`] to set a range, speed, prefix, suffix or decimal places.
    pub fn drag_value(&mut self, value: &mut f32, state: &mut DragValueState) -> DragValueResponse {
        self.drag_value_with(value, state, DragValueOptions::default())
    }

    /// Draws a drag value with explicit options.
    ///
    /// If both range bounds are finite, a reversed range is treated as its ascending equivalent
    /// and values are clamped to it. A range with a non-finite bound is ignored.
    pub fn drag_value_with(
        &mut self,
        value: &mut f32,
        state: &mut DragValueState,
        options: DragValueOptions,
    ) -> DragValueResponse {
        let range = finite_range(&options.range);
        let drag_style = self.ctx.style().drag_value.clone();
        let spacing = self.spacing();
        let available = available_size(&self.layout, spacing);
        let font = self.ctx.style().font.clone();
        let padding = Vec2::new(drag_style.padding.x.max(0.0), drag_style.padding.y.max(0.0));

        let display_text = format_value(*value, &options);
        let text_size = self
            .ctx
            .text_cache
            .layout(&self.ctx.text_system, &display_text, &font, None)
            .size();
        let size = Vec2::new(
            (text_size.x + padding.x * 2.0)
                .max(drag_style.min_size.x.max(0.0))
                .min(available.x.max(0.0)),
            (text_size.y + padding.y * 2.0).max(drag_style.min_size.y.max(0.0)),
        );

        // Editing uses the regular number input, restyled to the drag value's size and colors. The
        // width is measured from the displayed value so entering edit mode does not move the
        // surrounding layout.
        if state.edit.is_focused() {
            state.drag = None;
            let old_value = *value;
            let response = self.with_style(
                |style| {
                    style.number_input.min_size = size;
                    style.text_edit.padding = drag_style.padding;
                    style.text_edit.normal = drag_style.normal;
                    style.text_edit.hovered = drag_style.hovered;
                    style.text_edit.focused = drag_style.dragged;
                    style.text_edit.border = drag_style.border;
                    style.text_edit.border_width = drag_style.border_width;
                    style.text_edit.corner_radius = drag_style.corner_radius;
                    style.text_edit.text = drag_style.text;
                    style.text_edit.cursor = drag_style.text;
                },
                |ui| ui.number_input(value, &mut state.edit),
            );

            // Only user edits are clamped: focusing an out-of-range value with
            // `clamp_existing_to_range(false)` must not change it before anything is typed.
            if range.is_some() && (response.changed || response.submitted) {
                *value = clamp_value(*value, range);
            }
            let changed = *value != old_value;
            // Enter and blur normalize the text. When a range is active, do it ourselves even if
            // the value ended up back where it started: the field may still show the unclamped
            // number that was just typed.
            if (response.submitted || !response.focused) && range.is_some() {
                state.edit.reset_to_value(*value);
            }

            return DragValueResponse {
                rect: response.rect,
                hovered: response.hovered,
                focused: response.focused,
                changed,
                dragged: false,
                submitted: response.submitted,
            };
        }

        let rect = allocate_space(&mut self.layout, size, spacing);

        let hit = interaction(self.clip, &self.ctx.input, rect);
        if hit.hovered {
            self.set_cursor_icon(CursorIcon::ResizeHorizontal);
        }

        let mut changed = false;
        if options.clamp_existing_to_range
            && let Some((min, max)) = range
        {
            let clamped = clamp_value(*value, Some((min, max)));
            if clamped != *value {
                *value = clamped;
                changed = true;
            }
        }

        let pointer = self.ctx.input.pointer;
        let primary_down = self.ctx.input.primary_down;
        let speed = sanitized_speed(options.speed);
        let slow = if self.ctx.input.modifiers.shift {
            0.1
        } else {
            1.0
        };
        let was_dragging = state.drag.is_some_and(|drag| drag.moved);

        if hit.pressed
            && let Some(pointer) = pointer
        {
            state.drag = Some(DragStart {
                pointer,
                value: if value.is_finite() { *value } else { 0.0 },
                accumulated: 0.0,
                moved: false,
            });
        }

        if primary_down {
            if let Some(drag) = state.drag.as_mut()
                && let Some(pointer) = pointer
            {
                let delta = pointer - drag.pointer;
                drag.pointer = pointer;
                let delta_points = delta.x - delta.y;

                if drag.moved {
                    drag.value += delta_points * speed * slow;
                } else {
                    drag.accumulated += delta_points;
                    if drag.accumulated.abs() >= DRAG_THRESHOLD {
                        drag.moved = true;
                        drag.value += drag.accumulated * speed * slow;
                        drag.accumulated = 0.0;
                    }
                }

                if drag.moved && speed > 0.0 {
                    let clamped = clamp_value(drag.value, range);
                    drag.value = clamped;
                    if clamped != *value {
                        *value = clamped;
                        changed = true;
                    }
                }
            }
        } else {
            let released = state.drag.take();
            if self.ctx.input.primary_released
                && let Some(drag) = released
                && !drag.moved
                && pointer.is_some_and(|pointer| rect.contains(pointer))
            {
                state.edit.begin_editing(*value);
            }
        }

        if is_visible(&self.ctx.painter, rect) {
            let fill = if state.drag.is_some_and(|drag| drag.moved) || was_dragging {
                drag_style.dragged
            } else if hit.hovered {
                drag_style.hovered
            } else {
                drag_style.normal
            };

            self.ctx.painter.add(Shape::Rect(
                RectShape::new(rect, fill)
                    .with_corner_radius(drag_style.corner_radius)
                    .with_stroke(Stroke::new(
                        drag_style.border_width.max(0.0),
                        drag_style.border,
                    )),
            ));

            let display_text = if changed {
                format_value(*value, &options)
            } else {
                display_text
            };
            let layout =
                self.ctx
                    .text_cache
                    .layout(&self.ctx.text_system, &display_text, &font, None);
            paint_text_rect(
                &mut self.ctx.text_system,
                &mut self.ctx.painter,
                self.ctx.pixels_per_point,
                layout,
                rect,
                Align2::CENTER,
                drag_style.text,
            );
        }

        DragValueResponse {
            rect,
            hovered: hit.hovered,
            focused: state.edit.is_focused(),
            changed,
            dragged: state.drag.is_some_and(|drag| drag.moved),
            submitted: false,
        }
    }
}

/// Returns finite bounds in ascending order. Non-finite bounds disable the range.
fn finite_range(range: &Option<RangeInclusive<f32>>) -> Option<(f32, f32)> {
    let range = range.as_ref()?;
    let start = *range.start();
    let end = *range.end();
    if !start.is_finite() || !end.is_finite() {
        return None;
    }
    Some(if start <= end {
        (start, end)
    } else {
        (end, start)
    })
}

fn clamp_value(value: f32, range: Option<(f32, f32)>) -> f32 {
    match range {
        Some((min, max)) => {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                min
            }
        }
        None => value,
    }
}

fn sanitized_speed(speed: f32) -> f32 {
    if speed.is_finite() && speed > 0.0 {
        speed
    } else {
        0.0
    }
}

/// Formats `value` with the options' prefix, suffix and decimal places.
fn format_value(value: f32, options: &DragValueOptions) -> String {
    let number = format_number(value, options);
    if options.prefix.is_empty() && options.suffix.is_empty() {
        number
    } else {
        format!("{}{number}{}", options.prefix, options.suffix)
    }
}

fn format_number(value: f32, options: &DragValueOptions) -> String {
    // Formatting turns `-0.0` into `-0`, which is distracting while dragging through zero.
    let value = if value == 0.0 { 0.0 } else { value };
    if !value.is_finite() {
        return value.to_string();
    }

    let min_decimals = options.min_decimals.min(MAX_DECIMALS);
    let max_decimals = options
        .max_decimals
        .map(|max| max.min(MAX_DECIMALS).max(min_decimals));

    let mut text = match max_decimals {
        Some(max_decimals) => format!("{value:.max_decimals$}"),
        None => value.to_string(),
    };

    if max_decimals.is_some() {
        // Fixed formatting pads with zeros. Trim them down to the requested minimum so `1.5` does
        // not become `1.500000`, while `with_fixed_decimals(2)` still shows `1.50`.
        if text.contains('.') {
            while text.ends_with('0') && decimal_places(&text) > min_decimals {
                text.pop();
            }
            if text.ends_with('.') {
                text.pop();
            }
        }
    } else if min_decimals > 0 {
        let decimals = decimal_places(&text);
        if decimals == 0 {
            text.push('.');
        }
        for _ in decimals..min_decimals {
            text.push('0');
        }
    }

    text
}

fn decimal_places(text: &str) -> usize {
    text.split_once('.')
        .map_or(0, |(_, decimals)| decimals.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_util::context;
    use crate::{CursorIcon, Key, RawInput, pos2, vec2};

    fn options() -> DragValueOptions {
        DragValueOptions::default().with_speed(1.0)
    }

    #[test]
    fn drag_value_dragging_right_increases_the_value() {
        let mut context = context();
        let mut value = 10.0;
        let mut state = DragValueState::new();
        let mut rect = Rect::NOTHING;

        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            rect = ui.drag_value(&mut value, &mut state).rect;
        });

        let start = rect.center();
        let press = RawInput {
            pointer: Some(start),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, press, |ui| {
            ui.drag_value(&mut value, &mut state);
        });

        let drag = RawInput {
            pointer: Some(start + vec2(20.0, 0.0)),
            primary_down: true,
            ..Default::default()
        };
        let mut changed = false;
        let _ = context.run(vec2(300.0, 100.0), 1.0, drag, |ui| {
            changed = ui.drag_value(&mut value, &mut state).changed;
        });

        assert!(changed);
        assert_eq!(value, 30.0);
        assert!(!state.is_focused());
    }

    #[test]
    fn drag_value_dragging_up_increases_the_value_and_shift_slows_it_down() {
        let mut context = context();
        let mut value = 0.0;
        let mut state = DragValueState::new();
        let mut rect = Rect::NOTHING;

        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            rect = ui.drag_value(&mut value, &mut state).rect;
        });

        let start = rect.center();
        let press = RawInput {
            pointer: Some(start),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, press, |ui| {
            ui.drag_value(&mut value, &mut state);
        });

        let drag = RawInput {
            pointer: Some(start - vec2(0.0, 20.0)),
            primary_down: true,
            modifiers: crate::Modifiers {
                shift: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, drag, |ui| {
            ui.drag_value(&mut value, &mut state);
        });

        assert_eq!(value, 2.0);
    }

    #[test]
    fn drag_value_clamps_dragging_to_the_range() {
        let mut context = context();
        let mut value = 5.0;
        let mut state = DragValueState::new();
        let mut rect = Rect::NOTHING;
        let options = options().with_range(0.0..=10.0);

        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            rect = ui
                .drag_value_with(&mut value, &mut state, options.clone())
                .rect;
        });

        let start = rect.center();
        let press = RawInput {
            pointer: Some(start),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, press, |ui| {
            ui.drag_value_with(&mut value, &mut state, options.clone());
        });

        let drag = RawInput {
            pointer: Some(start + vec2(100.0, 0.0)),
            primary_down: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, drag, |ui| {
            ui.drag_value_with(&mut value, &mut state, options.clone());
        });

        assert_eq!(value, 10.0);
    }

    #[test]
    fn drag_value_click_opens_a_text_editor_and_commits_on_enter() {
        let mut context = context();
        let mut value = 42.0;
        let mut state = DragValueState::new();
        let mut rect = Rect::NOTHING;

        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            rect = ui.drag_value(&mut value, &mut state).rect;
        });

        let center = rect.center();
        let press = RawInput {
            pointer: Some(center),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, press, |ui| {
            ui.drag_value(&mut value, &mut state);
        });

        let release = RawInput {
            pointer: Some(center),
            primary_released: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, release, |ui| {
            ui.drag_value(&mut value, &mut state);
        });
        assert!(state.is_focused());
        assert_eq!(value, 42.0);

        // The editor is shown on the next frame. Typing replaces the selected number.
        let typed = RawInput {
            text: "12.5".to_owned(),
            ..Default::default()
        };
        let mut focused = false;
        let _ = context.run(vec2(300.0, 100.0), 1.0, typed, |ui| {
            focused = ui.drag_value(&mut value, &mut state).focused;
        });
        assert!(focused);
        assert_eq!(value, 12.5);

        let enter = RawInput {
            keys: vec![Key::Enter],
            ..Default::default()
        };
        let mut submitted = false;
        let _ = context.run(vec2(300.0, 100.0), 1.0, enter, |ui| {
            submitted = ui.drag_value(&mut value, &mut state).submitted;
        });
        assert!(submitted);
        assert_eq!(value, 12.5);
    }

    #[test]
    fn drag_value_editing_applies_the_range() {
        let mut context = context();
        let mut value = 5.0;
        let mut state = DragValueState::new();
        let mut rect = Rect::NOTHING;
        let options = options().with_range(0.0..=10.0);

        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            rect = ui
                .drag_value_with(&mut value, &mut state, options.clone())
                .rect;
        });
        let center = rect.center();
        let press = RawInput {
            pointer: Some(center),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, press, |ui| {
            ui.drag_value_with(&mut value, &mut state, options.clone());
        });
        let release = RawInput {
            pointer: Some(center),
            primary_released: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, release, |ui| {
            ui.drag_value_with(&mut value, &mut state, options.clone());
        });

        let typed = RawInput {
            text: "99".to_owned(),
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, typed, |ui| {
            ui.drag_value_with(&mut value, &mut state, options.clone());
        });
        assert_eq!(value, 10.0);

        let enter = RawInput {
            keys: vec![Key::Enter],
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, enter, |ui| {
            ui.drag_value_with(&mut value, &mut state, options.clone());
        });
        assert_eq!(value, 10.0);
        assert_eq!(state.edit.text(), "10");
    }

    #[test]
    fn drag_value_dragging_does_not_open_the_editor() {
        let mut context = context();
        let mut value = 0.0;
        let mut state = DragValueState::new();
        let mut rect = Rect::NOTHING;

        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            rect = ui.drag_value(&mut value, &mut state).rect;
        });
        let start = rect.center();
        let press = RawInput {
            pointer: Some(start),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, press, |ui| {
            ui.drag_value(&mut value, &mut state);
        });
        let drag = RawInput {
            pointer: Some(start + vec2(20.0, 0.0)),
            primary_down: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, drag, |ui| {
            ui.drag_value(&mut value, &mut state);
        });
        let release = RawInput {
            pointer: Some(start + vec2(20.0, 0.0)),
            primary_released: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, release, |ui| {
            ui.drag_value(&mut value, &mut state);
        });

        assert!(!state.is_focused());
    }

    #[test]
    fn drag_value_requests_a_horizontal_resize_cursor_while_hovered() {
        let mut context = context();
        let mut value = 0.0;
        let mut state = DragValueState::new();
        let mut rect = Rect::NOTHING;

        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            rect = ui.drag_value(&mut value, &mut state).rect;
        });

        let hover = RawInput {
            pointer: Some(rect.center()),
            ..Default::default()
        };
        let frame = context.run(vec2(300.0, 100.0), 1.0, hover, |ui| {
            ui.drag_value(&mut value, &mut state);
        });

        assert_eq!(frame.platform.cursor_icon, CursorIcon::ResizeHorizontal);
    }

    #[test]
    fn drag_value_clamps_an_existing_value_by_default() {
        let mut context = context();
        let mut value = 50.0;
        let mut state = DragValueState::new();

        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.drag_value_with(&mut value, &mut state, options().with_range(0.0..=10.0));
        });

        assert_eq!(value, 10.0);
    }

    #[test]
    fn drag_value_can_leave_an_existing_out_of_range_value_alone() {
        let mut context = context();
        let mut value = 50.0;
        let mut state = DragValueState::new();

        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.drag_value_with(
                &mut value,
                &mut state,
                options()
                    .with_range(0.0..=10.0)
                    .clamp_existing_to_range(false),
            );
        });

        assert_eq!(value, 50.0);
    }

    #[test]
    fn drag_value_focus_does_not_clamp_without_an_edit() {
        let mut context = context();
        let mut value = 50.0;
        let mut state = DragValueState::new();
        let mut rect = Rect::NOTHING;
        let options = options()
            .with_range(0.0..=10.0)
            .clamp_existing_to_range(false);

        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            rect = ui
                .drag_value_with(&mut value, &mut state, options.clone())
                .rect;
        });
        let center = rect.center();
        let press = RawInput {
            pointer: Some(center),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, press, |ui| {
            ui.drag_value_with(&mut value, &mut state, options.clone());
        });
        let release = RawInput {
            pointer: Some(center),
            primary_released: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, release, |ui| {
            ui.drag_value_with(&mut value, &mut state, options.clone());
        });
        assert!(state.is_focused());

        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.drag_value_with(&mut value, &mut state, options.clone());
        });
        assert_eq!(value, 50.0);
    }

    #[test]
    fn drag_value_formats_prefix_suffix_and_decimals() {
        let options = DragValueOptions::default()
            .with_prefix("x = ")
            .with_suffix(" px")
            .with_fixed_decimals(2);

        assert_eq!(format_value(1.5, &options), "x = 1.50 px");
        assert_eq!(format_value(-0.0, &options), "x = 0.00 px");
        assert_eq!(
            format_value(1.26, &options.clone().with_fixed_decimals(1)),
            "x = 1.3 px"
        );
        assert_eq!(
            format_value(
                1.2000,
                &options.clone().with_min_decimals(1).with_max_decimals(3)
            ),
            "x = 1.2 px"
        );
        assert_eq!(
            format_value(
                2.0,
                &options
                    .clone()
                    .with_max_decimals_opt(None)
                    .with_min_decimals(1)
            ),
            "x = 2.0 px"
        );
    }

    #[test]
    fn drag_value_ignores_a_non_finite_range() {
        assert_eq!(finite_range(&Some(0.0..=f32::INFINITY)), None);
        assert_eq!(finite_range(&Some(1.0..=0.0)), Some((0.0, 1.0)));
    }

    #[test]
    fn drag_value_starts_a_drag_on_an_empty_pointer_position() {
        // An app can build a frame without any pointer at all; that must not panic or start a
        // stale drag after the pointer comes back.
        let mut context = context();
        let mut value = 1.0;
        let mut state = DragValueState::new();

        let press = RawInput {
            pointer: Some(pos2(1.0, 1.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, press, |ui| {
            ui.drag_value(&mut value, &mut state);
        });
        let _ = context.run(vec2(300.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.drag_value(&mut value, &mut state);
        });

        assert!(!state.is_dragging());
        assert!(!state.is_focused());
    }
}
