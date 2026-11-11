//! Numeric text inputs built on the single-line text editor.

use tinim_paint::Rect;

use super::super::Ui;
use super::text_edit::TextEditState;

/// Caller-owned state for [`Ui::number_input`](crate::Ui::number_input).
///
/// The widget keeps the text being edited here so partially typed values such as `-` or `1.`
/// survive between frames. The value itself is passed separately as `&mut f32`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NumberInputState {
    pub(crate) text: String,
    pub(crate) edit: TextEditState,
    /// Whether the user changed the text since it gained focus.
    pub(crate) edited: bool,
}

impl NumberInputState {
    /// A blurred input that will format the value on the next frame.
    pub const fn new() -> Self {
        Self {
            text: String::new(),
            edit: TextEditState::new(),
            edited: false,
        }
    }

    /// Whether the input currently owns keyboard input.
    pub fn is_focused(&self) -> bool {
        self.edit.is_focused()
    }

    /// The text currently shown in the input.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Focuses or blurs the input.
    ///
    /// Blurring collapses the text selection. The next frame formats the text from the value.
    pub fn set_focused(&mut self, focused: bool) {
        self.edit.set_focused(focused);
    }
}

/// The result of building a [`Ui::number_input`](crate::Ui::number_input).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumberInputResponse {
    pub rect: Rect,
    pub hovered: bool,
    pub focused: bool,
    /// The numeric value changed during this frame.
    pub changed: bool,
    /// Enter was pressed while the input was focused.
    pub submitted: bool,
}

impl Ui<'_> {
    /// Draws a numeric text input and updates `value` when the text parses as a finite number.
    ///
    /// The field shares [`TextEditStyle`](crate::TextEditStyle) with
    /// [`Ui::text_edit_singleline`], but its size comes from
    /// [`NumberInputStyle`](crate::NumberInputStyle). Clicking focuses it, Enter commits the value,
    /// and clicking elsewhere commits a valid number or restores the text from `value`.
    ///
    /// While the input is focused, external changes to `value` do not overwrite the text. Blurring
    /// commits the text only if the user edited it; otherwise the external value is formatted into
    /// the field.
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

        // The numeric field is usually much narrower than a full text editor, so its size is
        // independent even though the rest of the appearance is shared.
        let min_size = self.ctx.style().number_input.min_size;
        let response = self.with_style(
            |style| style.text_edit.min_size = min_size,
            |ui| ui.text_edit_singleline(&mut state.text, &mut state.edit),
        );

        if !was_focused && response.focused {
            state.edited = false;
        }

        let mut changed = false;
        let mut submitted = false;

        if response.changed {
            state.edited = true;
            if let Some(parsed) = parse_number(&state.text)
                && parsed != *value
            {
                *value = parsed;
                changed = true;
            }
        }

        if response.submitted {
            submitted = true;
            if state.edited
                && let Some(parsed) = parse_number(&state.text)
                && parsed != *value
            {
                *value = parsed;
                changed = true;
            }
            state.text = format_number(*value);
            state.edited = false;
        }

        // A click outside blurs the editor. Commit an edited number, otherwise leave an external
        // value change intact and simply refresh the text from it.
        if was_focused && !response.focused {
            if state.edited
                && let Some(parsed) = parse_number(&state.text)
                && parsed != *value
            {
                *value = parsed;
                changed = true;
            }
            state.text = format_number(*value);
            state.edited = false;
        }

        NumberInputResponse {
            rect: response.rect,
            hovered: response.hovered,
            focused: response.focused,
            changed,
            submitted,
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_util::context;
    use crate::{Key, RawInput, pos2, vec2};

    #[test]
    fn number_input_uses_a_compact_width_independent_of_the_text_editor() {
        let mut context = context();
        context.style_mut().text_edit.min_size.x = 400.0;
        let expected = context.style().number_input.min_size.x;
        let mut value = 0.0;
        let mut state = NumberInputState::new();
        let mut rect = Rect::NOTHING;

        let _ = context.run(vec2(500.0, 100.0), 1.0, RawInput::default(), |ui| {
            rect = ui.number_input(&mut value, &mut state).rect;
        });

        assert_eq!(rect.width(), expected);
        assert!(rect.width() < 400.0);
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
    fn number_input_blur_keeps_an_external_value_when_nothing_was_typed() {
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

        let outside = RawInput {
            pointer: Some(pos2(500.0, 500.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(300.0, 100.0), 1.0, outside, |ui| {
            ui.number_input(&mut value, &mut state);
        });

        assert_eq!(value, 9.0);
        assert_eq!(state.text(), "9");
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
}
