//! Single-line text editing.

use std::ops::Range;

use tinim_paint::{Rect, RectShape, Shape, Stroke, Vec2, pos2, vec2};
use unicode_segmentation::UnicodeSegmentation;

use super::super::Ui;
use super::super::layout::{allocate_space, available_size, pointer_over};
use crate::cursor::CursorIcon;
use crate::input::Key;

/// Caller-owned state for [`Ui::text_edit_singleline`](crate::Ui::text_edit_singleline).
///
/// The caret and selection are byte indices into the edited string and are kept on character
/// boundaries. Use [`TextEditState::new`] or [`Default`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextEditState {
    /// Whether the editor currently owns keyboard input.
    ///
    /// Clicking the editor focuses it and clicking anywhere else blurs it. Applications can also
    /// set this directly to move focus programmatically.
    pub focused: bool,
    /// Caret position as a byte index into the text.
    pub(crate) cursor: usize,
    /// The other end of the selection, also a byte index.
    pub(crate) anchor: usize,
    /// Horizontal scroll offset in logical points.
    pub(crate) scroll: f32,
    /// Seconds since the caret last moved, used for blinking.
    pub(crate) blink: f32,
    /// Whether a pointer drag is extending the selection.
    pub(crate) dragging: bool,
}

impl TextEditState {
    /// A blurred editor with the caret at the start and no selection.
    pub const fn new() -> Self {
        Self {
            focused: false,
            cursor: 0,
            anchor: 0,
            scroll: 0.0,
            blink: 0.0,
            dragging: false,
        }
    }

    /// Caret position as a byte index into the text.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Selected byte range. Empty when there is no selection.
    pub fn selection(&self) -> Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
    }

    /// Current horizontal scroll offset in logical points.
    pub fn scroll(&self) -> f32 {
        self.scroll
    }

    /// Whether the editor currently owns keyboard input.
    pub fn is_focused(&self) -> bool {
        self.focused
    }

    /// Focuses or blurs the editor. Blurring collapses the selection.
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.blink = 0.0;
        if !focused {
            self.anchor = self.cursor;
            self.dragging = false;
        }
    }
}

/// The result of building a [`Ui::text_edit_singleline`](crate::Ui::text_edit_singleline).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextEditResponse {
    pub rect: Rect,
    pub hovered: bool,
    pub focused: bool,
    /// The text changed during this frame.
    pub changed: bool,
    /// Enter was pressed while the editor was focused.
    pub submitted: bool,
}

impl Ui<'_> {
    /// Draws a single-line text editor.
    ///
    /// `state` is caller-owned. The editor takes keyboard input while `state.focused` is true.
    /// Clicking inside focuses it and clicking outside blurs it, so several editors built in the
    /// same frame behave as one focus group without widget ids.
    ///
    /// Enter does not insert a newline; it reports [`TextEditResponse::submitted`] instead. The
    /// caller should keep `text` free of line breaks.
    pub fn text_edit_singleline(
        &mut self,
        text: &mut String,
        state: &mut TextEditState,
    ) -> TextEditResponse {
        let edit_style = self.ctx.style().text_edit.clone();
        let font = self.ctx.style().font.clone();
        let line_height = self.ctx.text_system.line_height(&font);
        let spacing = self.spacing();
        let available = available_size(&self.layout, spacing);
        let size = Vec2::new(
            available.x.min(edit_style.min_size.x).max(0.0),
            (line_height + edit_style.padding.y * 2.0).max(edit_style.min_size.y),
        );
        let rect = allocate_space(&mut self.layout, size, spacing);
        let inner = Rect::from_min_size(
            rect.min + edit_style.padding,
            (rect.size() - edit_style.padding * 2.0).max(Vec2::ZERO),
        );

        clamp_state(text, state);

        let hovered = pointer_over(self.clip, &self.ctx.input, rect);
        if hovered {
            self.set_cursor_icon(CursorIcon::Text);
        }
        let pressed_inside = hovered && self.ctx.input.primary_pressed;
        let pointer_pressed = self.ctx.input.primary_pressed;

        // Hit testing needs the current layout. Editing below invalidates it, so this layout is
        // only used to resolve the pointer position.
        let pointer_cursor =
            if pressed_inside || (state.focused && state.dragging && self.ctx.input.primary_down) {
                let layout = self
                    .ctx
                    .text_cache
                    .layout(&self.ctx.text_system, text, &font, None);
                let x = self
                    .ctx
                    .input
                    .pointer
                    .map(|pointer| pointer.x - inner.min.x + state.scroll)
                    .unwrap_or(0.0);
                layout
                    .lines()
                    .first()
                    .map(|line| line.byte_index_at_x(x))
                    .unwrap_or(0)
            } else {
                0
            };

        if pressed_inside {
            state.focused = true;
            state.dragging = true;
            state.blink = 0.0;
            state.cursor = pointer_cursor;
            state.anchor = pointer_cursor;
        } else if pointer_pressed {
            if state.focused {
                state.focused = false;
                state.anchor = state.cursor;
            }
            state.dragging = false;
        } else if state.focused && state.dragging && self.ctx.input.primary_down {
            state.cursor = pointer_cursor;
            state.blink = 0.0;
        }
        if !self.ctx.input.primary_down {
            state.dragging = false;
        }

        let mut changed = false;
        let mut submitted = false;

        if state.focused {
            let command = self.ctx.input.command();
            let shift = self.ctx.input.modifiers.shift;
            let keys = std::mem::take(&mut self.ctx.input.keys);

            for key in keys {
                match key {
                    Key::Backspace => changed |= delete_previous(text, state),
                    Key::Delete => changed |= delete_next(text, state),
                    Key::ArrowLeft => move_left(text, state, shift),
                    Key::ArrowRight => move_right(text, state, shift),
                    Key::Home => move_home(state, shift),
                    Key::End => move_end(text, state, shift),
                    Key::Character('a') if command => {
                        state.anchor = 0;
                        state.cursor = text.len();
                        state.blink = 0.0;
                    }
                    Key::Character('c') if command => {
                        if let Some(copied) = copy_selection(text, state) {
                            self.ctx.platform.copy_text = Some(copied);
                        }
                    }
                    Key::Character('x') if command => {
                        if let Some(copied) = copy_selection(text, state) {
                            self.ctx.platform.copy_text = Some(copied);
                            replace_selection(text, state, "");
                            changed = true;
                        }
                    }
                    Key::Character('v') if command => {
                        changed |= paste_clipboard(text, state, &mut self.ctx.input.pasted_text);
                    }
                    Key::Enter => submitted = true,
                    Key::Escape => {
                        state.focused = false;
                        state.anchor = state.cursor;
                        state.dragging = false;
                    }
                    Key::Tab | Key::Character(_) => {}
                }
            }

            // Browsers read the clipboard asynchronously, so paste text may arrive after the
            // Ctrl/Cmd+V key was already consumed. Apply it even without the key in this frame.
            if state.focused {
                changed |= paste_clipboard(text, state, &mut self.ctx.input.pasted_text);
            }

            // Command shortcuts may also produce text on some platforms. Only plain typing
            // inserts characters.
            if state.focused && !command {
                let typed = std::mem::take(&mut self.ctx.input.text);
                let typed: String = typed.chars().filter(|ch| !ch.is_control()).collect();
                if !typed.is_empty() {
                    replace_selection(text, state, &typed);
                    changed = true;
                }
            }
        }

        let layout = self
            .ctx
            .text_cache
            .layout(&self.ctx.text_system, text, &font, None);
        let text_height = if layout.lines().is_empty() {
            line_height
        } else {
            layout.height()
        };
        let cursor_x = layout
            .lines()
            .first()
            .map(|line| line.cursor_x(state.cursor))
            .unwrap_or(0.0);
        // `inner` can be taller than the text (for example when `min_size.y` exceeds the line
        // height plus padding). Center the line box the same way `button` centers its text.
        let text_top = inner.min.y + (inner.height() - text_height).max(0.0) * 0.5;
        let text_height = text_height.min(inner.height());

        let max_scroll = (layout.width() - inner.width()).max(0.0);
        state.scroll = state.scroll.clamp(0.0, max_scroll);
        if state.focused {
            if cursor_x < state.scroll {
                state.scroll = cursor_x;
            } else if cursor_x > state.scroll + inner.width() {
                state.scroll = cursor_x - inner.width();
            }
            state.scroll = state.scroll.clamp(0.0, max_scroll);
            state.blink = (state.blink + self.ctx.input.dt.max(0.0)) % 1.0;
        }

        let clip = self.ctx.painter.clip_rect();
        if clip.intersect(rect).is_positive() {
            let fill = if state.focused {
                edit_style.focused
            } else if hovered {
                edit_style.hovered
            } else {
                edit_style.normal
            };
            self.ctx.painter.add(Shape::Rect(
                RectShape::new(rect, fill)
                    .with_corner_radius(edit_style.corner_radius)
                    .with_stroke(Stroke::new(edit_style.border_width, edit_style.border)),
            ));

            let inner_clip = clip.intersect(inner);
            if inner_clip.is_positive() {
                self.ctx.painter.set_clip_rect(inner_clip);

                let selection = state.selection();
                if !selection.is_empty()
                    && let Some(line) = layout.lines().first()
                {
                    for glyph in line.glyphs() {
                        let cluster = glyph.cluster();
                        if cluster.end <= selection.start || cluster.start >= selection.end {
                            continue;
                        }
                        let selection_rect = Rect::from_min_size(
                            pos2(inner.min.x + glyph.x() - state.scroll, text_top),
                            vec2(glyph.advance(), text_height),
                        );
                        self.ctx.painter.rect(selection_rect, edit_style.selection);
                    }
                }

                if !layout.lines().is_empty() {
                    let origin = pos2(inner.min.x - state.scroll, text_top);
                    let mesh = self.ctx.text_system.prepare(
                        layout,
                        origin,
                        edit_style.text,
                        self.ctx.pixels_per_point,
                    );
                    self.ctx.painter.add_mesh(mesh);
                }

                if state.focused && state.blink < 0.5 {
                    let caret = Rect::from_min_size(
                        pos2(inner.min.x + cursor_x - state.scroll, text_top),
                        vec2(edit_style.cursor_width, text_height),
                    );
                    self.ctx.painter.rect(caret, edit_style.cursor);
                }

                self.ctx.painter.set_clip_rect(clip);
            }
        }

        TextEditResponse {
            rect,
            hovered,
            focused: state.focused,
            changed,
            submitted,
        }
    }
}

fn clamp_state(text: &str, state: &mut TextEditState) {
    state.cursor = floor_boundary(text, state.cursor);
    state.anchor = floor_boundary(text, state.anchor);
}

fn floor_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn replace_selection(text: &mut String, state: &mut TextEditState, inserted: &str) {
    let selection = state.selection();
    text.replace_range(selection.clone(), inserted);
    state.cursor = selection.start + inserted.len();
    state.anchor = state.cursor;
    state.blink = 0.0;
}

fn copy_selection(text: &str, state: &TextEditState) -> Option<String> {
    let selection = state.selection();
    if selection.is_empty() {
        None
    } else {
        Some(text[selection].to_owned())
    }
}

/// Inserts clipboard text at the caret or over the selection.
fn paste_clipboard(
    text: &mut String,
    state: &mut TextEditState,
    pasted: &mut Option<String>,
) -> bool {
    let Some(pasted) = pasted.take() else {
        return false;
    };
    let pasted = sanitize_single_line(&pasted);
    if pasted.is_empty() {
        return false;
    }
    replace_selection(text, state, &pasted);
    true
}

/// Converts pasted text to a single line and drops control characters.
fn sanitize_single_line(text: &str) -> String {
    text.chars()
        .map(|ch| match ch {
            '\r' | '\n' | '\t' => ' ',
            ch => ch,
        })
        .filter(|ch| !ch.is_control())
        .collect()
}

fn delete_previous(text: &mut String, state: &mut TextEditState) -> bool {
    let selection = state.selection();
    if !selection.is_empty() {
        text.replace_range(selection.clone(), "");
        state.cursor = selection.start;
        state.anchor = selection.start;
        state.blink = 0.0;
        return true;
    }
    if state.cursor == 0 {
        return false;
    }
    let start = previous_grapheme_boundary(text, state.cursor);
    text.replace_range(start..state.cursor, "");
    state.cursor = start;
    state.anchor = start;
    state.blink = 0.0;
    true
}

fn delete_next(text: &mut String, state: &mut TextEditState) -> bool {
    let selection = state.selection();
    if !selection.is_empty() {
        text.replace_range(selection.clone(), "");
        state.cursor = selection.start;
        state.anchor = selection.start;
        state.blink = 0.0;
        return true;
    }
    if state.cursor >= text.len() {
        return false;
    }
    let end = next_grapheme_boundary(text, state.cursor);
    text.replace_range(state.cursor..end, "");
    state.anchor = state.cursor;
    state.blink = 0.0;
    true
}

fn move_left(text: &str, state: &mut TextEditState, extend: bool) {
    let selection = state.selection();
    let target = if !extend && !selection.is_empty() {
        selection.start
    } else {
        previous_grapheme_boundary(text, state.cursor)
    };
    set_cursor(state, target, extend);
}

fn move_right(text: &str, state: &mut TextEditState, extend: bool) {
    let selection = state.selection();
    let target = if !extend && !selection.is_empty() {
        selection.end
    } else {
        next_grapheme_boundary(text, state.cursor)
    };
    set_cursor(state, target, extend);
}

fn move_home(state: &mut TextEditState, extend: bool) {
    set_cursor(state, 0, extend);
}

fn move_end(text: &str, state: &mut TextEditState, extend: bool) {
    set_cursor(state, text.len(), extend);
}

/// Moves the caret and either keeps the anchor (extending the selection) or collapses it.
fn set_cursor(state: &mut TextEditState, target: usize, extend: bool) {
    state.cursor = target;
    if !extend {
        state.anchor = target;
    }
    state.blink = 0.0;
}

fn previous_grapheme_boundary(text: &str, index: usize) -> usize {
    text[..index]
        .grapheme_indices(true)
        .next_back()
        .map(|(offset, _)| offset)
        .unwrap_or(0)
}

fn next_grapheme_boundary(text: &str, index: usize) -> usize {
    text[index..]
        .graphemes(true)
        .next()
        .map(|grapheme| index + grapheme.len())
        .unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_util::context;
    use crate::{CursorIcon, Modifiers, RawInput, pos2, vec2};

    fn press() -> RawInput {
        RawInput {
            pointer: Some(pos2(1.0, 1.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        }
    }

    fn keys(keys: Vec<Key>, modifiers: Modifiers) -> RawInput {
        RawInput {
            keys,
            modifiers,
            ..Default::default()
        }
    }

    #[test]
    fn hovered_editor_requests_text_cursor() {
        let mut context = context();
        let mut text = String::new();
        let mut state = TextEditState::new();
        let hover = RawInput {
            pointer: Some(pos2(1.0, 1.0)),
            ..Default::default()
        };

        let frame = context.run(vec2(200.0, 100.0), 1.0, hover, |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });
        assert_eq!(frame.platform.cursor_icon, CursorIcon::Text);
    }

    #[test]
    fn focuses_and_inserts_text() {
        let mut context = context();
        let mut text = String::new();
        let mut state = TextEditState::new();
        let mut changed = false;

        let _ = context.run(vec2(200.0, 100.0), 1.0, press(), |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });
        assert!(state.is_focused());

        let input = RawInput {
            text: "hello".to_owned(),
            ..Default::default()
        };
        let _ = context.run(vec2(200.0, 100.0), 1.0, input, |ui| {
            changed = ui.text_edit_singleline(&mut text, &mut state).changed;
        });

        assert_eq!(text, "hello");
        assert!(changed);
        assert_eq!(state.cursor(), 5);
    }

    #[test]
    fn click_outside_blurs_and_collapses_the_selection() {
        let mut context = context();
        let mut text = "abc".to_owned();
        let mut state = TextEditState::new();

        let _ = context.run(vec2(200.0, 100.0), 1.0, press(), |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });
        let _ = context.run(
            vec2(200.0, 100.0),
            1.0,
            keys(vec![Key::End], Modifiers::default()),
            |ui| {
                ui.text_edit_singleline(&mut text, &mut state);
            },
        );
        let _ = context.run(
            vec2(200.0, 100.0),
            1.0,
            keys(
                vec![Key::ArrowLeft],
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
            ),
            |ui| {
                ui.text_edit_singleline(&mut text, &mut state);
            },
        );
        assert_eq!(state.selection(), 2..3);

        let outside = RawInput {
            pointer: Some(pos2(500.0, 500.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(200.0, 100.0), 1.0, outside, |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });

        assert!(!state.is_focused());
        assert!(state.selection().is_empty());
    }

    #[test]
    fn click_positions_the_caret() {
        let mut context = context();
        let mut text = "abc".to_owned();
        let mut state = TextEditState::new();

        // Padding is (8, 4) by default and Ahem advances one em per glyph (14 points).
        let input = RawInput {
            pointer: Some(pos2(8.0 + 30.0, 10.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        let _ = context.run(vec2(200.0, 100.0), 1.0, input, |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });

        assert_eq!(state.cursor(), 2);
    }

    #[test]
    fn caret_is_vertically_centered_in_the_editor() {
        let mut context = context();
        let mut text = "abc".to_owned();
        let mut state = TextEditState::new();
        let mut rect = None;

        let frame = context.run(vec2(200.0, 100.0), 1.0, press(), |ui| {
            rect = Some(ui.text_edit_singleline(&mut text, &mut state).rect);
        });

        let rect = rect.expect("editor was built");
        // The caret is the last shape drawn and therefore the last four vertices.
        let vertices = &frame.paint.vertices;
        let caret = &vertices[vertices.len() - 4..];
        let top = caret
            .iter()
            .map(|vertex| vertex.pos[1])
            .fold(f32::INFINITY, f32::min);
        let bottom = caret
            .iter()
            .map(|vertex| vertex.pos[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let caret_center = (top + bottom) * 0.5;

        assert!(
            (caret_center - rect.center().y).abs() < 0.01,
            "caret center {caret_center} should match editor center {}",
            rect.center().y
        );
    }

    #[test]
    fn backspace_deletes_graphemes() {
        let mut context = context();
        let mut text = String::new();
        let mut state = TextEditState::new();

        let _ = context.run(vec2(200.0, 100.0), 1.0, press(), |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });
        let input = RawInput {
            text: "a😀b".to_owned(),
            ..Default::default()
        };
        let _ = context.run(vec2(200.0, 100.0), 1.0, input, |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });

        for expected in ["a😀", "a"] {
            let _ = context.run(
                vec2(200.0, 100.0),
                1.0,
                keys(vec![Key::Backspace], Modifiers::default()),
                |ui| {
                    ui.text_edit_singleline(&mut text, &mut state);
                },
            );
            assert_eq!(text, expected);
        }
    }

    #[test]
    fn shift_arrows_select_and_typing_replaces_the_selection() {
        let mut context = context();
        let mut text = "abc".to_owned();
        let mut state = TextEditState::new();

        let _ = context.run(vec2(200.0, 100.0), 1.0, press(), |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });
        let _ = context.run(
            vec2(200.0, 100.0),
            1.0,
            keys(vec![Key::Home], Modifiers::default()),
            |ui| {
                ui.text_edit_singleline(&mut text, &mut state);
            },
        );
        let _ = context.run(
            vec2(200.0, 100.0),
            1.0,
            keys(
                vec![Key::ArrowRight],
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
            ),
            |ui| {
                ui.text_edit_singleline(&mut text, &mut state);
            },
        );
        assert_eq!(state.selection(), 0..1);

        let input = RawInput {
            text: "X".to_owned(),
            ..Default::default()
        };
        let _ = context.run(vec2(200.0, 100.0), 1.0, input, |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });

        assert_eq!(text, "Xbc");
        assert_eq!(state.cursor(), 1);
    }

    #[test]
    fn ctrl_a_selects_all() {
        let mut context = context();
        let mut text = "abc".to_owned();
        let mut state = TextEditState::new();

        let _ = context.run(vec2(200.0, 100.0), 1.0, press(), |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });
        let _ = context.run(
            vec2(200.0, 100.0),
            1.0,
            keys(
                vec![Key::Character('a')],
                Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            ),
            |ui| {
                ui.text_edit_singleline(&mut text, &mut state);
            },
        );

        assert_eq!(state.selection(), 0..3);
    }

    #[test]
    fn enter_reports_submission_without_inserting_a_newline() {
        let mut context = context();
        let mut text = "abc".to_owned();
        let mut state = TextEditState::new();
        let mut submitted = false;

        let _ = context.run(vec2(200.0, 100.0), 1.0, press(), |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });
        let _ = context.run(
            vec2(200.0, 100.0),
            1.0,
            keys(vec![Key::Enter], Modifiers::default()),
            |ui| {
                submitted = ui.text_edit_singleline(&mut text, &mut state).submitted;
            },
        );

        assert!(submitted);
        assert_eq!(text, "abc");
        assert!(state.is_focused());
    }

    #[test]
    fn end_scrolls_long_text_into_view() {
        let mut context = context();
        let mut text = "abcdefghijklmnopqrstuvwxyz".to_owned();
        let mut state = TextEditState::new();

        let _ = context.run(vec2(200.0, 100.0), 1.0, press(), |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });
        let _ = context.run(
            vec2(200.0, 100.0),
            1.0,
            keys(vec![Key::End], Modifiers::default()),
            |ui| {
                ui.text_edit_singleline(&mut text, &mut state);
            },
        );

        assert_eq!(state.cursor(), text.len());
        assert!(state.scroll() > 0.0);
    }

    #[test]
    fn ctrl_c_copies_the_selection_without_changing_the_text() {
        let mut context = context();
        let mut text = "hello".to_owned();
        let mut state = TextEditState::new();

        let _ = context.run(vec2(200.0, 100.0), 1.0, press(), |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });
        state.cursor = 0;
        state.anchor = 2;

        let frame = context.run(
            vec2(200.0, 100.0),
            1.0,
            keys(
                vec![Key::Character('c')],
                Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            ),
            |ui| {
                ui.text_edit_singleline(&mut text, &mut state);
            },
        );

        assert_eq!(frame.platform.copy_text.as_deref(), Some("he"));
        assert_eq!(text, "hello");
    }

    #[test]
    fn ctrl_c_does_not_copy_an_empty_selection() {
        let mut context = context();
        let mut text = "hello".to_owned();
        let mut state = TextEditState::new();

        let _ = context.run(vec2(200.0, 100.0), 1.0, press(), |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });

        let frame = context.run(
            vec2(200.0, 100.0),
            1.0,
            keys(
                vec![Key::Character('c')],
                Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            ),
            |ui| {
                ui.text_edit_singleline(&mut text, &mut state);
            },
        );

        assert_eq!(frame.platform.copy_text, None);
        assert_eq!(text, "hello");
    }

    #[test]
    fn ctrl_x_copies_and_deletes_the_selection() {
        let mut context = context();
        let mut text = "hello".to_owned();
        let mut state = TextEditState::new();

        let _ = context.run(vec2(200.0, 100.0), 1.0, press(), |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });
        state.cursor = 0;
        state.anchor = 2;
        let mut changed = false;

        let frame = context.run(
            vec2(200.0, 100.0),
            1.0,
            keys(
                vec![Key::Character('x')],
                Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            ),
            |ui| {
                changed = ui.text_edit_singleline(&mut text, &mut state).changed;
            },
        );

        assert!(changed);
        assert_eq!(text, "llo");
        assert_eq!(state.cursor(), 0);
        assert_eq!(frame.platform.copy_text.as_deref(), Some("he"));
    }

    #[test]
    fn ctrl_v_pastes_at_the_caret() {
        let mut context = context();
        let mut text = "hello".to_owned();
        let mut state = TextEditState::new();

        let _ = context.run(vec2(200.0, 100.0), 1.0, press(), |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });
        state.cursor = text.len();
        state.anchor = text.len();
        let mut changed = false;

        let input = RawInput {
            keys: vec![Key::Character('v')],
            modifiers: Modifiers {
                ctrl: true,
                ..Default::default()
            },
            pasted_text: Some(" world".to_owned()),
            ..Default::default()
        };
        let _ = context.run(vec2(200.0, 100.0), 1.0, input, |ui| {
            changed = ui.text_edit_singleline(&mut text, &mut state).changed;
        });

        assert!(changed);
        assert_eq!(text, "hello world");
        assert_eq!(state.cursor(), text.len());
    }

    #[test]
    fn ctrl_v_replaces_the_selection_and_normalizes_the_line() {
        let mut context = context();
        let mut text = "hello".to_owned();
        let mut state = TextEditState::new();

        let _ = context.run(vec2(200.0, 100.0), 1.0, press(), |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });
        state.cursor = 0;
        state.anchor = 2;

        let input = RawInput {
            keys: vec![Key::Character('v')],
            modifiers: Modifiers {
                ctrl: true,
                ..Default::default()
            },
            pasted_text: Some("a\nb\tc".to_owned()),
            ..Default::default()
        };
        let _ = context.run(vec2(200.0, 100.0), 1.0, input, |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });

        assert_eq!(text, "a b cllo");
        assert_eq!(state.cursor(), 5);
    }

    #[test]
    fn pasted_text_can_arrive_without_the_shortcut_key() {
        // Browser clipboard reads are asynchronous, so the text may arrive a frame after the
        // Ctrl/Cmd+V key was consumed.
        let mut context = context();
        let mut text = "hello".to_owned();
        let mut state = TextEditState::new();

        let _ = context.run(vec2(200.0, 100.0), 1.0, press(), |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });
        state.cursor = text.len();
        state.anchor = text.len();
        let mut changed = false;

        let input = RawInput {
            pasted_text: Some(" world".to_owned()),
            ..Default::default()
        };
        let _ = context.run(vec2(200.0, 100.0), 1.0, input, |ui| {
            changed = ui.text_edit_singleline(&mut text, &mut state).changed;
        });

        assert!(changed);
        assert_eq!(text, "hello world");
    }

    #[test]
    fn clipboard_shortcuts_are_ignored_while_unfocused() {
        let mut context = context();
        let mut text = "hello".to_owned();
        let mut state = TextEditState::new();

        let input = RawInput {
            keys: vec![Key::Character('v')],
            modifiers: Modifiers {
                ctrl: true,
                ..Default::default()
            },
            pasted_text: Some("world".to_owned()),
            ..Default::default()
        };
        let frame = context.run(vec2(200.0, 100.0), 1.0, input, |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });

        assert_eq!(text, "hello");
        assert_eq!(frame.platform.copy_text, None);
    }
}
