//! Single-line and multi-line text editing.

use std::borrow::Cow;
use std::ops::Range;

use tinim_paint::{Rect, RectShape, Shape, Stroke, Vec2, pos2, vec2};
use tinim_text::{TextLayout, TextStyle};
use unicode_segmentation::UnicodeSegmentation;

use super::super::Ui;
use super::super::layout::{allocate_space, available_size, pointer_over};
use crate::cursor::CursorIcon;
use crate::input::Key;
use crate::platform::ImeOutput;

/// Caller-owned state for [`Ui::text_edit_singleline`](crate::Ui::text_edit_singleline) and
/// [`Ui::text_edit_multiline`](crate::Ui::text_edit_multiline).
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
    /// Vertical scroll offset in logical points. Only multi-line editors use it.
    pub(crate) scroll_y: f32,
    /// Horizontal caret position remembered while moving between lines with the arrow keys.
    pub(crate) desired_x: Option<f32>,
    /// Caret position seen on the previous frame, used to reveal it after it moves.
    pub(crate) last_cursor: usize,
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
            scroll_y: 0.0,
            desired_x: None,
            last_cursor: 0,
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

    /// Current vertical scroll offset in logical points.
    ///
    /// This is always zero for single-line editors. Multi-line editors change it when the caret
    /// moves outside the visible area or when the pointer wheel scrolls over them.
    pub fn scroll_y(&self) -> f32 {
        self.scroll_y
    }

    /// Whether the editor currently owns keyboard input.
    pub fn is_focused(&self) -> bool {
        self.focused
    }

    /// Focuses or blurs the editor. Blurring collapses the selection.
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.blink = 0.0;
        self.desired_x = None;
        if !focused {
            self.anchor = self.cursor;
            self.dragging = false;
        }
    }
}

/// The result of building a text editor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextEditResponse {
    pub rect: Rect,
    pub hovered: bool,
    pub focused: bool,
    /// The text changed during this frame.
    pub changed: bool,
    /// Enter was pressed while the editor was focused.
    ///
    /// Single-line editors report Enter here instead of inserting a newline. Multi-line editors
    /// insert a newline and never report a submission, so this is always false for them.
    pub submitted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextEditKind {
    SingleLine,
    Multiline,
}

impl Ui<'_> {
    /// Draws a single-line text editor.
    ///
    /// `state` is caller-owned. The editor takes keyboard input while `state.focused` is true.
    /// Clicking inside focuses it and clicking outside blurs it, so several editors built in the
    /// same frame behave as one focus group without widget ids.
    ///
    /// Enter does not insert a newline; it reports [`TextEditResponse::submitted`] instead. The
    /// caller should keep `text` free of line breaks. Use [`Ui::text_edit_multiline`] for editable
    /// text that may contain them.
    ///
    /// IME composition is supported through [`RawInput::ime`](crate::RawInput::ime). The preedit
    /// is drawn inline and underlined without modifying `text`; committed text is inserted at the
    /// caret. While focused, the editor reports its caret area through
    /// [`PlatformOutput::ime`](crate::PlatformOutput::ime) so the backend can enable IME input and
    /// place the candidate window.
    ///
    /// The backend must supply the IME events. Native winit does; winit 0.30's web backend does
    /// not, so browsers need a separate DOM bridge to open a soft keyboard or forward composition.
    pub fn text_edit_singleline(
        &mut self,
        text: &mut String,
        state: &mut TextEditState,
    ) -> TextEditResponse {
        self.text_edit(text, state, TextEditKind::SingleLine)
    }

    /// Draws a multi-line text editor.
    ///
    /// The editor wraps text at its inner width, starts at the top of its rectangle and scrolls
    /// vertically. Its rectangle comes from [`TextEditStyle::multiline_min_size`](crate::TextEditStyle::multiline_min_size)
    /// and is capped horizontally by the available space.
    ///
    /// Enter inserts a newline and never reports [`TextEditResponse::submitted`]. Up and Down move
    /// the caret between visual lines, and Home and End move it to the start or end of the current
    /// visual line. Holding Shift extends the selection. Pasting text keeps line breaks and
    /// normalizes CRLF and lone CR to `\n`.
    ///
    /// Clicking inside focuses the editor, clicking outside blurs it, and IME composition works
    /// like it does for [`Ui::text_edit_singleline`].
    pub fn text_edit_multiline(
        &mut self,
        text: &mut String,
        state: &mut TextEditState,
    ) -> TextEditResponse {
        self.text_edit(text, state, TextEditKind::Multiline)
    }

    fn text_edit(
        &mut self,
        text: &mut String,
        state: &mut TextEditState,
        kind: TextEditKind,
    ) -> TextEditResponse {
        let multiline = kind == TextEditKind::Multiline;
        let edit_style = self.ctx.style().text_edit.clone();
        let font = self.ctx.style().font.clone();
        let line_height = self.ctx.text_system.line_height(&font);
        let spacing = self.spacing();
        let available = available_size(&self.layout, spacing);
        let min_size = if multiline {
            edit_style.multiline_min_size
        } else {
            edit_style.min_size
        };
        let mut size = Vec2::new(available.x.min(min_size.x).max(0.0), min_size.y.max(0.0));
        let minimum_height = line_height + edit_style.padding.y * 2.0;
        size.y = if multiline {
            size.y.max(minimum_height)
        } else {
            minimum_height.max(min_size.y)
        };
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
        let wrap_width = multiline.then_some(inner.width());
        let pointer_cursor =
            if pressed_inside || (state.focused && state.dragging && self.ctx.input.primary_down) {
                let layout =
                    self.ctx
                        .text_cache
                        .layout(&self.ctx.text_system, text, &font, wrap_width);
                let (x, y) = self
                    .ctx
                    .input
                    .pointer
                    .map(|pointer| {
                        (
                            pointer.x - inner.min.x + state.scroll,
                            pointer.y - inner.min.y + state.scroll_y,
                        )
                    })
                    .unwrap_or((0.0, 0.0));
                if multiline {
                    let line = if line_height > 0.0 {
                        (y / line_height).floor().max(0.0) as usize
                    } else {
                        0
                    };
                    let line = line.min(visual_line_count(layout, text).saturating_sub(1));
                    line_byte_index_at_x(layout, text, line, x)
                } else {
                    layout
                        .lines()
                        .first()
                        .map(|line| line.byte_index_at_x(x))
                        .unwrap_or(0)
                }
            } else {
                0
            };

        if pressed_inside {
            state.focused = true;
            state.dragging = true;
            state.blink = 0.0;
            state.desired_x = None;
            state.cursor = pointer_cursor;
            state.anchor = pointer_cursor;
        } else if pointer_pressed {
            if state.focused {
                state.focused = false;
                state.anchor = state.cursor;
            }
            state.dragging = false;
            state.desired_x = None;
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
            self.ctx.platform.text_input_focused = true;
            let command = self.ctx.input.command();
            let shift = self.ctx.input.modifiers.shift;
            let keys = std::mem::take(&mut self.ctx.input.keys);

            for key in keys {
                match key {
                    Key::Backspace => changed |= delete_previous(text, state),
                    Key::Delete => changed |= delete_next(text, state),
                    Key::ArrowLeft => move_left(text, state, shift),
                    Key::ArrowRight => move_right(text, state, shift),
                    Key::ArrowUp => {
                        if multiline {
                            self.move_vertical(text, state, &font, wrap_width, false, shift);
                        }
                    }
                    Key::ArrowDown => {
                        if multiline {
                            self.move_vertical(text, state, &font, wrap_width, true, shift);
                        }
                    }
                    Key::Home => {
                        if multiline {
                            self.move_to_visual_edge(text, state, &font, wrap_width, false, shift);
                        } else {
                            move_home(state, shift);
                        }
                    }
                    Key::End => {
                        if multiline {
                            self.move_to_visual_edge(text, state, &font, wrap_width, true, shift);
                        } else {
                            move_end(text, state, shift);
                        }
                    }
                    Key::Character('a') if command => {
                        state.anchor = 0;
                        state.cursor = text.len();
                        state.desired_x = None;
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
                        changed |= paste_clipboard(
                            text,
                            state,
                            &mut self.ctx.input.pasted_text,
                            multiline,
                        );
                    }
                    Key::Enter => {
                        if multiline {
                            replace_selection(text, state, "\n");
                            changed = true;
                        } else {
                            submitted = true;
                        }
                    }
                    Key::Escape => {
                        state.focused = false;
                        state.anchor = state.cursor;
                        state.dragging = false;
                        state.desired_x = None;
                    }
                    Key::Tab | Key::Character(_) => {}
                }
            }

            // Browsers read the clipboard asynchronously, so paste text may arrive after the
            // Ctrl/Cmd+V key was already consumed. Apply it even without the key in this frame.
            if state.focused {
                changed |= paste_clipboard(text, state, &mut self.ctx.input.pasted_text, multiline);
            }

            // Insert text committed by an IME. It replaces the selection just like typed text.
            if state.focused {
                let committed = sanitize_input(&self.ctx.input.ime.commit, multiline);
                if !committed.is_empty() {
                    replace_selection(text, state, &committed);
                    changed = true;
                }
            }

            // Command shortcuts may also produce text on some platforms. Only plain typing
            // inserts characters.
            if state.focused && !command {
                let typed = std::mem::take(&mut self.ctx.input.text);
                let typed = if multiline {
                    sanitize_input(&typed, true)
                } else {
                    typed.chars().filter(|ch| !ch.is_control()).collect()
                };
                if !typed.is_empty() {
                    replace_selection(text, state, &typed);
                    changed = true;
                }
            }
        }

        // While the IME composes, draw the preedit in place of the selection without changing the
        // edited string. `caret` is a byte index into `display_text`.
        let selection = state.selection();
        let composing = state.focused && self.ctx.input.ime.is_composing();
        let (display_text, visible_selection, preedit, caret) = if composing {
            let preedit = &self.ctx.input.ime.preedit;
            let mut composed = String::with_capacity(text.len() + preedit.len());
            composed.push_str(&text[..selection.start]);
            let preedit_start = composed.len();
            composed.push_str(preedit);
            let preedit_end = composed.len();
            composed.push_str(&text[selection.end..]);
            let caret_in_preedit = self
                .ctx
                .input
                .ime
                .preedit_selection
                .as_ref()
                .map(|range| floor_boundary(preedit, range.end.min(preedit.len())))
                .unwrap_or(preedit.len());
            (
                Cow::Owned(composed),
                preedit_start..preedit_start,
                Some(preedit_start..preedit_end),
                preedit_start + caret_in_preedit,
            )
        } else {
            (Cow::Borrowed(text.as_str()), selection, None, state.cursor)
        };

        let layout =
            self.ctx
                .text_cache
                .layout(&self.ctx.text_system, &display_text, &font, wrap_width);
        let line_count = visual_line_count(layout, &display_text);
        let content_height = line_count as f32 * line_height;
        let caret_line = if multiline {
            visual_line_index(layout, &display_text, caret)
        } else {
            0
        };
        let cursor_x = if multiline {
            visual_cursor_x(layout, caret_line, caret)
        } else {
            layout
                .lines()
                .first()
                .map(|line| line.cursor_x(caret))
                .unwrap_or(0.0)
        };

        if multiline {
            state.scroll = 0.0;
            let max_scroll = (content_height - inner.height()).max(0.0);
            state.scroll_y = state.scroll_y.clamp(0.0, max_scroll);

            // Wheel input is applied here so the editor can consume what it needs and leave the
            // rest for an enclosing scrollable. It intentionally ignores touch drags, which
            // extend the text selection instead.
            if hovered && !self.ctx.input.scroll_dragging && self.scroll_remaining.y != 0.0 {
                let desired = state.scroll_y - self.scroll_remaining.y;
                state.scroll_y = desired.clamp(0.0, max_scroll);
                let unused = desired - state.scroll_y;
                self.scroll_remaining.y = -unused;
            }

            // Reveal the caret when it moves without fighting the user's wheel scrolling.
            if state.focused && state.cursor != state.last_cursor {
                let caret_top = caret_line as f32 * line_height;
                if caret_top < state.scroll_y {
                    state.scroll_y = caret_top;
                } else if caret_top + line_height > state.scroll_y + inner.height() {
                    state.scroll_y = caret_top + line_height - inner.height();
                }
                state.scroll_y = state.scroll_y.clamp(0.0, max_scroll);
            }
        } else {
            let max_scroll = (layout.width() - inner.width()).max(0.0);
            state.scroll = state.scroll.clamp(0.0, max_scroll);
            if state.focused {
                if cursor_x < state.scroll {
                    state.scroll = cursor_x;
                } else if cursor_x > state.scroll + inner.width() {
                    state.scroll = cursor_x - inner.width();
                }
                state.scroll = state.scroll.clamp(0.0, max_scroll);
            }
            state.scroll_y = 0.0;
        }
        state.last_cursor = state.cursor;
        if state.focused {
            state.blink = (state.blink + self.ctx.input.dt.max(0.0)) % 1.0;
        }

        let raw_text_height = if multiline || layout.lines().is_empty() {
            line_height
        } else {
            layout.height()
        };
        // `inner` can be taller than the text (for example when `min_size.y` exceeds the line
        // height plus padding). A single-line editor centers the line box like `button` centers
        // its text. A multi-line editor starts at the top and scrolls.
        let text_top = if multiline {
            inner.min.y - state.scroll_y
        } else {
            inner.min.y + (inner.height() - raw_text_height).max(0.0) * 0.5
        };
        let text_height = if multiline {
            line_height
        } else {
            raw_text_height.min(inner.height())
        };
        let caret_top = if multiline {
            text_top + caret_line as f32 * line_height
        } else {
            text_top
        };

        // The platform places the IME candidate window at the caret. Report the same rectangle
        // that the caret is drawn at, including the current scroll offsets.
        let ime_cursor_rect = Rect::from_min_size(
            pos2(inner.min.x + cursor_x - state.scroll, caret_top),
            vec2(edit_style.cursor_width.max(1.0), text_height),
        );

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

                if !visible_selection.is_empty() {
                    for (index, line) in layout.lines().iter().enumerate() {
                        let range = line.range();
                        if visible_selection.end <= range.start
                            || visible_selection.start >= range.end
                        {
                            continue;
                        }
                        let top = if multiline {
                            text_top + index as f32 * line_height
                        } else {
                            text_top
                        };
                        for glyph in line.glyphs() {
                            let cluster = glyph.cluster();
                            if cluster.end <= visible_selection.start
                                || cluster.start >= visible_selection.end
                            {
                                continue;
                            }
                            let selection_rect = Rect::from_min_size(
                                pos2(inner.min.x + glyph.x() - state.scroll, top),
                                vec2(glyph.advance(), text_height),
                            );
                            self.ctx.painter.rect(selection_rect, edit_style.selection);
                        }
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

                // Underline the composition string so it is distinguishable from committed text.
                if let Some(preedit) = preedit.as_ref() {
                    for (index, line) in layout.lines().iter().enumerate() {
                        let range = line.range();
                        let start = preedit.start.max(range.start);
                        let end = preedit.end.min(range.end);
                        if start >= end {
                            continue;
                        }
                        let start_x = line.cursor_x(start);
                        let end_x = line.cursor_x(end);
                        if end_x > start_x {
                            let bottom = if multiline {
                                text_top + (index as f32 + 1.0) * line_height - 1.0
                            } else {
                                text_top + text_height - 1.0
                            };
                            let underline = Rect::from_min_size(
                                pos2(inner.min.x + start_x - state.scroll, bottom),
                                vec2(end_x - start_x, 1.0),
                            );
                            self.ctx.painter.rect(underline, edit_style.text);
                        }
                    }
                }

                if state.focused && (composing || state.blink < 0.5) {
                    self.ctx.painter.rect(ime_cursor_rect, edit_style.cursor);
                }

                self.ctx.painter.set_clip_rect(clip);
            }
        }

        if state.focused {
            self.ctx.platform.ime = Some(ImeOutput {
                cursor_rect: ime_cursor_rect,
            });
        }

        TextEditResponse {
            rect,
            hovered,
            focused: state.focused,
            changed,
            submitted,
        }
    }

    /// Moves the caret to the start or end of the visual line that contains it.
    fn move_to_visual_edge(
        &mut self,
        text: &str,
        state: &mut TextEditState,
        font: &TextStyle,
        max_width: Option<f32>,
        end: bool,
        extend: bool,
    ) {
        let layout = self
            .ctx
            .text_cache
            .layout(&self.ctx.text_system, text, font, max_width);
        let index = visual_line_index(layout, text, state.cursor);
        let range = visual_line_range(layout, text, index);
        set_cursor(state, if end { range.end } else { range.start }, extend);
    }

    /// Moves the caret one visual line up or down, preserving its horizontal position.
    fn move_vertical(
        &mut self,
        text: &str,
        state: &mut TextEditState,
        font: &TextStyle,
        max_width: Option<f32>,
        down: bool,
        extend: bool,
    ) {
        let layout = self
            .ctx
            .text_cache
            .layout(&self.ctx.text_system, text, font, max_width);
        let count = visual_line_count(layout, text);
        let index = visual_line_index(layout, text, state.cursor);
        let target_index = if down {
            (index + 1).min(count.saturating_sub(1))
        } else {
            index.saturating_sub(1)
        };
        if target_index == index {
            return;
        }

        let x = state
            .desired_x
            .unwrap_or_else(|| visual_cursor_x(layout, index, state.cursor));
        let target = line_byte_index_at_x(layout, text, target_index, x);
        state.cursor = target;
        if !extend {
            state.anchor = target;
        }
        state.desired_x = Some(x);
        state.blink = 0.0;
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
    state.desired_x = None;
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
    multiline: bool,
) -> bool {
    let Some(pasted) = pasted.take() else {
        return false;
    };
    let pasted = sanitize_input(&pasted, multiline);
    if pasted.is_empty() {
        return false;
    }
    replace_selection(text, state, &pasted);
    true
}

/// Sanitizes input for an editor, dropping control characters it cannot display.
fn sanitize_input(text: &str, multiline: bool) -> String {
    if multiline {
        sanitize_multiline(text)
    } else {
        sanitize_single_line(text)
    }
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

/// Normalizes line endings and drops control characters that are not line breaks or tabs.
fn sanitize_multiline(text: &str) -> String {
    let mut sanitized = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                sanitized.push('\n');
            }
            '\u{2028}' | '\u{2029}' => sanitized.push('\n'),
            '\n' | '\t' => sanitized.push(ch),
            ch if ch.is_control() => {}
            ch => sanitized.push(ch),
        }
    }
    sanitized
}

fn delete_previous(text: &mut String, state: &mut TextEditState) -> bool {
    let selection = state.selection();
    if !selection.is_empty() {
        text.replace_range(selection.clone(), "");
        state.cursor = selection.start;
        state.anchor = selection.start;
        state.desired_x = None;
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
    state.desired_x = None;
    state.blink = 0.0;
    true
}

fn delete_next(text: &mut String, state: &mut TextEditState) -> bool {
    let selection = state.selection();
    if !selection.is_empty() {
        text.replace_range(selection.clone(), "");
        state.cursor = selection.start;
        state.anchor = selection.start;
        state.desired_x = None;
        state.blink = 0.0;
        return true;
    }
    if state.cursor >= text.len() {
        return false;
    }
    let end = next_grapheme_boundary(text, state.cursor);
    text.replace_range(state.cursor..end, "");
    state.anchor = state.cursor;
    state.desired_x = None;
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
    state.desired_x = None;
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

/// Number of visual lines, including the empty line after a trailing line break.
fn visual_line_count(layout: &TextLayout, text: &str) -> usize {
    layout.line_count() + usize::from(has_trailing_empty_line(text))
}

/// Index of the visual line that contains `byte`.
///
/// A caret at the boundary between soft-wrapped lines belongs to the following line, while a
/// caret before an explicit line break belongs to the current one. A position after a trailing
/// line break belongs to the appended empty line.
fn visual_line_index(layout: &TextLayout, text: &str, byte: usize) -> usize {
    let mut index = 0;
    for (line_index, line) in layout.lines().iter().enumerate() {
        if line.range().start <= byte {
            index = line_index;
        } else {
            break;
        }
    }
    if has_trailing_empty_line(text) && byte >= text.len() {
        index = layout.line_count();
    }
    index
}

/// Byte range of a visual line. The appended empty line after a trailing break is `text.len()..text.len()`.
fn visual_line_range(layout: &TextLayout, text: &str, index: usize) -> Range<usize> {
    layout
        .lines()
        .get(index)
        .map(|line| line.range())
        .unwrap_or(text.len()..text.len())
}

fn visual_cursor_x(layout: &TextLayout, index: usize, byte: usize) -> f32 {
    match layout.lines().get(index) {
        // Wrapped layouts trim trailing spaces from the line, so positions after the range are at
        // the visible end of the line instead of inside its last glyph.
        Some(line) if byte > line.range().end => line.width(),
        Some(line) => line.cursor_x(byte),
        None => 0.0,
    }
}

/// Converts an x position on a visual line to a byte index.
///
/// `TextLine::byte_index_at_x` returns zero for an empty line because it has no glyphs; the line's
/// own byte position is the correct answer there.
fn line_byte_index_at_x(layout: &TextLayout, text: &str, index: usize, x: f32) -> usize {
    match layout.lines().get(index) {
        Some(line) if !line.glyphs().is_empty() => line.byte_index_at_x(x),
        Some(line) => line.range().start,
        None => text.len(),
    }
}

fn has_trailing_empty_line(text: &str) -> bool {
    text.ends_with(['\n', '\r', '\u{000B}', '\u{000C}'])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_util::context;
    use crate::{CursorIcon, ImeInput, Modifiers, RawInput, pos2, vec2};

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

    fn ime(preedit: &str, preedit_selection: Option<Range<usize>>, commit: &str) -> RawInput {
        RawInput {
            ime: ImeInput {
                preedit: preedit.to_owned(),
                preedit_selection,
                commit: commit.to_owned(),
            },
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
    fn focused_editor_reports_that_a_text_input_has_focus() {
        let mut context = context();
        let mut text = String::new();
        let mut state = TextEditState::new();

        let frame = context.run(vec2(200.0, 100.0), 1.0, press(), |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });

        assert!(state.is_focused());
        assert!(frame.platform.text_input_focused);
        assert!(
            frame.platform.ime.is_some(),
            "a focused editor should allow IME input"
        );
    }

    #[test]
    fn unfocused_editor_does_not_report_text_input_focus() {
        let mut context = context();
        let mut text = String::new();
        let mut state = TextEditState::new();

        let frame = context.run(vec2(200.0, 100.0), 1.0, RawInput::default(), |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });

        assert!(!state.is_focused());
        assert!(!frame.platform.text_input_focused);
        assert!(frame.platform.ime.is_none());
    }

    #[test]
    fn ime_preedit_is_shown_without_changing_the_text() {
        let mut context = context();
        let mut text = "abc".to_owned();
        let mut state = TextEditState::new();

        let _ = context.run(vec2(200.0, 100.0), 1.0, press(), |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });
        state.cursor = text.len();
        state.anchor = text.len();

        let committed = context
            .run(vec2(200.0, 100.0), 1.0, RawInput::default(), |ui| {
                ui.text_edit_singleline(&mut text, &mut state);
            })
            .platform
            .ime
            .expect("a focused editor reports IME state")
            .cursor_rect
            .min
            .x;

        let frame = context.run(vec2(200.0, 100.0), 1.0, ime("zz", Some(2..2), ""), |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });

        assert_eq!(text, "abc", "preedit is not inserted before a commit");
        assert_eq!(state.cursor(), text.len());
        let composing = frame
            .platform
            .ime
            .expect("composing text still reports IME state")
            .cursor_rect
            .min
            .x;
        assert!(
            composing > committed,
            "the caret and IME candidate window follow the preedit"
        );
    }

    #[test]
    fn ime_preedit_selection_positions_the_caret() {
        let mut context = context();
        let mut text = String::new();
        let mut state = TextEditState::new();

        let _ = context.run(vec2(200.0, 100.0), 1.0, press(), |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });

        let start = context
            .run(vec2(200.0, 100.0), 1.0, ime("abcd", Some(0..0), ""), |ui| {
                ui.text_edit_singleline(&mut text, &mut state);
            })
            .platform
            .ime
            .expect("a focused editor reports IME state")
            .cursor_rect
            .min
            .x;
        let middle = context
            .run(vec2(200.0, 100.0), 1.0, ime("abcd", Some(1..3), ""), |ui| {
                ui.text_edit_singleline(&mut text, &mut state);
            })
            .platform
            .ime
            .expect("a focused editor reports IME state")
            .cursor_rect
            .min
            .x;

        assert_eq!(text, "");
        // Ahem advances one em per character, so moving from the start to the end of a
        // three-byte selection advances by three characters.
        assert!(
            middle > start,
            "preedit cursor end should move the caret: start={start}, middle={middle}"
        );
    }

    #[test]
    fn ime_commit_inserts_text_at_the_caret() {
        let mut context = context();
        let mut text = "hello".to_owned();
        let mut state = TextEditState::new();
        let mut changed = false;

        let _ = context.run(vec2(200.0, 100.0), 1.0, press(), |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });
        state.cursor = text.len();
        state.anchor = text.len();

        let _ = context.run(vec2(200.0, 100.0), 1.0, ime("", None, " world"), |ui| {
            changed = ui.text_edit_singleline(&mut text, &mut state).changed;
        });

        assert!(changed);
        assert_eq!(text, "hello world");
        assert_eq!(state.cursor(), text.len());
    }

    #[test]
    fn ime_commit_replaces_the_selection_and_normalizes_the_line() {
        let mut context = context();
        let mut text = "hello".to_owned();
        let mut state = TextEditState::new();

        let _ = context.run(vec2(200.0, 100.0), 1.0, press(), |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });
        state.cursor = 0;
        state.anchor = 2;

        let _ = context.run(vec2(200.0, 100.0), 1.0, ime("", None, "a\nb\tc"), |ui| {
            ui.text_edit_singleline(&mut text, &mut state);
        });

        assert_eq!(text, "a b cllo");
        assert_eq!(state.cursor(), 5);
    }

    #[test]
    fn ime_commit_is_ignored_while_unfocused() {
        let mut context = context();
        let mut text = "hello".to_owned();
        let mut state = TextEditState::new();
        let mut changed = false;

        let _ = context.run(vec2(200.0, 100.0), 1.0, ime("", None, "world"), |ui| {
            changed = ui.text_edit_singleline(&mut text, &mut state).changed;
        });

        assert!(!changed);
        assert_eq!(text, "hello");
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

    fn run_multiline(
        context: &mut crate::Context,
        text: &mut String,
        state: &mut TextEditState,
        min_size: Option<Vec2>,
        input: RawInput,
    ) -> (crate::TextEditResponse, crate::PlatformOutput) {
        let mut response = None;
        let frame = context.run(vec2(300.0, 300.0), 1.0, input, |ui| {
            if let Some(min_size) = min_size {
                ui.with_style(
                    |style| style.text_edit.multiline_min_size = min_size,
                    |ui| {
                        response = Some(ui.text_edit_multiline(text, state));
                    },
                );
            } else {
                response = Some(ui.text_edit_multiline(text, state));
            }
        });
        (response.expect("the editor was built"), frame.platform)
    }

    fn focus_multiline(context: &mut crate::Context, text: &mut String, state: &mut TextEditState) {
        run_multiline(context, text, state, None, press());
        assert!(state.is_focused());
    }

    #[test]
    fn multiline_enter_inserts_a_newline() {
        let mut context = context();
        let mut text = String::new();
        let mut state = TextEditState::new();
        focus_multiline(&mut context, &mut text, &mut state);

        let (response, _) = run_multiline(
            &mut context,
            &mut text,
            &mut state,
            None,
            keys(vec![Key::Enter], Modifiers::default()),
        );

        assert!(response.changed);
        assert!(!response.submitted);
        assert_eq!(text, "\n");
        assert_eq!(state.cursor(), 1);
    }

    #[test]
    fn multiline_arrow_keys_follow_lines_and_keep_the_column() {
        let mut context = context();
        let mut text = "abc\ndef".to_owned();
        let mut state = TextEditState::new();
        focus_multiline(&mut context, &mut text, &mut state);
        state.cursor = text.len();
        state.anchor = text.len();

        run_multiline(
            &mut context,
            &mut text,
            &mut state,
            None,
            keys(vec![Key::ArrowUp], Modifiers::default()),
        );
        assert_eq!(state.cursor(), 3);

        run_multiline(
            &mut context,
            &mut text,
            &mut state,
            None,
            keys(vec![Key::ArrowDown], Modifiers::default()),
        );
        assert_eq!(state.cursor(), 7);
    }

    #[test]
    fn multiline_shift_arrow_down_extends_the_selection() {
        let mut context = context();
        let mut text = "abc\ndef".to_owned();
        let mut state = TextEditState::new();
        focus_multiline(&mut context, &mut text, &mut state);

        run_multiline(
            &mut context,
            &mut text,
            &mut state,
            None,
            keys(
                vec![Key::ArrowDown],
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
            ),
        );

        assert_eq!(state.cursor(), 4);
        assert_eq!(state.selection(), 0..4);
    }

    #[test]
    fn multiline_home_and_end_use_the_current_line() {
        let mut context = context();
        let mut text = "abc\ndef".to_owned();
        let mut state = TextEditState::new();
        focus_multiline(&mut context, &mut text, &mut state);
        state.cursor = 5;
        state.anchor = 5;

        run_multiline(
            &mut context,
            &mut text,
            &mut state,
            None,
            keys(vec![Key::Home], Modifiers::default()),
        );
        assert_eq!(state.cursor(), 4);

        run_multiline(
            &mut context,
            &mut text,
            &mut state,
            None,
            keys(vec![Key::End], Modifiers::default()),
        );
        assert_eq!(state.cursor(), 7);
    }

    #[test]
    fn multiline_click_positions_the_caret_on_the_clicked_line() {
        let mut context = context();
        let mut text = "abc\ndef".to_owned();
        let mut state = TextEditState::new();

        // Padding is (8, 4) and Ahem advances one em (14 points) per glyph, so 30 points into the
        // second line, whose top is 4 + 14, is inside the third glyph.
        let input = RawInput {
            pointer: Some(pos2(8.0 + 30.0, 4.0 + 14.0 + 7.0)),
            primary_down: true,
            primary_pressed: true,
            ..Default::default()
        };
        run_multiline(&mut context, &mut text, &mut state, None, input);

        assert!(state.is_focused());
        assert_eq!(state.cursor(), 6);
    }

    #[test]
    fn multiline_paste_keeps_line_breaks_and_normalizes_crlf() {
        let mut context = context();
        let mut text = String::new();
        let mut state = TextEditState::new();
        focus_multiline(&mut context, &mut text, &mut state);

        let input = RawInput {
            pasted_text: Some("a\r\nb\tc".to_owned()),
            ..Default::default()
        };
        let (response, _) = run_multiline(&mut context, &mut text, &mut state, None, input);

        assert!(response.changed);
        assert_eq!(text, "a\nb\tc");
        assert_eq!(state.cursor(), text.len());
    }

    #[test]
    fn multiline_ime_commit_keeps_line_breaks() {
        let mut context = context();
        let mut text = String::new();
        let mut state = TextEditState::new();
        focus_multiline(&mut context, &mut text, &mut state);

        run_multiline(
            &mut context,
            &mut text,
            &mut state,
            None,
            ime("", None, "a\nb"),
        );

        assert_eq!(text, "a\nb");
        assert_eq!(state.cursor(), 3);
    }

    #[test]
    fn multiline_vertical_arrows_follow_wrapped_lines() {
        let mut context = context();
        let mut text = "abcd".to_owned();
        let mut state = TextEditState::new();
        let size = Some(vec2(48.0, 60.0));

        run_multiline(&mut context, &mut text, &mut state, size, press());
        state.cursor = text.len();
        state.anchor = text.len();

        run_multiline(
            &mut context,
            &mut text,
            &mut state,
            size,
            keys(vec![Key::ArrowUp], Modifiers::default()),
        );

        // The inner width is 32 points, so "ab" and "cd" are separate visual lines.
        assert_eq!(state.cursor(), 2);
    }

    #[test]
    fn multiline_scrolls_to_reveal_the_caret() {
        let mut context = context();
        let mut text = "a\nb\nc".to_owned();
        let mut state = TextEditState::new();
        let size = Some(vec2(240.0, 22.0));

        run_multiline(&mut context, &mut text, &mut state, size, press());
        state.cursor = text.len();
        state.anchor = text.len();

        run_multiline(
            &mut context,
            &mut text,
            &mut state,
            size,
            RawInput::default(),
        );

        // The viewport shows one line, so revealing the third line scrolls by two lines.
        assert!((state.scroll_y() - 28.0).abs() < 0.01);
    }

    #[test]
    fn multiline_trailing_newline_caret_uses_an_appended_line() {
        let mut context = context();
        let mut text = "a\n".to_owned();
        let mut state = TextEditState::new();
        focus_multiline(&mut context, &mut text, &mut state);

        state.cursor = 1;
        state.anchor = 1;
        let (_, platform) = run_multiline(
            &mut context,
            &mut text,
            &mut state,
            None,
            RawInput::default(),
        );
        let first = platform
            .ime
            .expect("a focused editor reports IME state")
            .cursor_rect
            .min
            .y;

        state.cursor = 2;
        state.anchor = 2;
        let (_, platform) = run_multiline(
            &mut context,
            &mut text,
            &mut state,
            None,
            RawInput::default(),
        );
        let appended = platform
            .ime
            .expect("a focused editor reports IME state")
            .cursor_rect
            .min
            .y;

        assert_eq!(state.cursor(), 2);
        assert!(
            appended > first,
            "the caret after a trailing newline is on the next line"
        );
    }

    #[test]
    fn multiline_caret_after_trailing_spaces_stays_at_the_line_end() {
        let mut context = context();
        let mut text = "ab   \ncd".to_owned();
        let mut state = TextEditState::new();
        focus_multiline(&mut context, &mut text, &mut state);
        state.cursor = 5;
        state.anchor = 5;

        let (_, platform) = run_multiline(
            &mut context,
            &mut text,
            &mut state,
            None,
            RawInput::default(),
        );
        let caret_x = platform
            .ime
            .expect("a focused editor reports IME state")
            .cursor_rect
            .min
            .x;

        // Wrapping trims the spaces from the line, so the caret sits after "ab", not before it.
        assert!((caret_x - (8.0 + 28.0)).abs() < 0.01);
    }

    #[test]
    fn multiline_wheel_scrolls_the_text() {
        let mut context = context();
        let mut text = "a\nb\nc\nd".to_owned();
        let mut state = TextEditState::new();
        let size = Some(vec2(240.0, 36.0));

        run_multiline(&mut context, &mut text, &mut state, size, press());
        let wheel = RawInput {
            pointer: Some(pos2(10.0, 10.0)),
            scroll: vec2(0.0, -28.0),
            ..Default::default()
        };
        run_multiline(&mut context, &mut text, &mut state, size, wheel);

        assert_eq!(state.scroll_y(), 28.0);
    }
}
