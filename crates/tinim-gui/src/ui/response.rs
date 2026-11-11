//! Results and caller-owned state exchanged with widgets.

use std::ops::Range;

use tinim_paint::{Rect, Vec2};

/// What a widget reports for this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Response {
    pub rect: Rect,
    pub hovered: bool,
    /// The primary button was pressed on this widget during this frame.
    pub pressed: bool,
    /// The primary button is currently held over this widget.
    pub down: bool,
}

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

/// The result of building a [`Ui::pane`](crate::Ui::pane).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneResponse {
    /// The viewport rectangle allocated in the parent flow.
    pub rect: Rect,
    /// Extent of the in-flow content, independent of the pane's offset.
    ///
    /// Out-of-flow children such as [`Ui::place_at`](crate::Ui::place_at) and
    /// [`Ui::place_relative`](crate::Ui::place_relative) are not included.
    pub content_size: Vec2,
}

/// Caller-owned state for [`Ui::scrollable`](crate::Ui::scrollable).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollState {
    /// Current scroll position, clamped to the content bounds. Zero shows the content origin.
    pub offset: Vec2,
    /// Content size measured on the previous frame, if any.
    ///
    /// When this is known, the scroll position is clamped before layout, so an out-of-range
    /// position is corrected in the same frame. It is `None` on the first frame.
    pub content_size: Option<Vec2>,
    /// Scroll velocity in logical points per second, in the same axes as [`Self::offset`].
    pub velocity: Vec2,
    /// Whether a touch drag currently owns the scroll position.
    pub dragging: bool,
}

impl ScrollState {
    /// A scroll state at the content origin with no active animation.
    pub const fn new() -> Self {
        Self {
            offset: Vec2::ZERO,
            content_size: None,
            velocity: Vec2::ZERO,
            dragging: false,
        }
    }
}

/// The result of building a [`Ui::scrollable`](crate::Ui::scrollable).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollResponse {
    /// The viewport rectangle allocated in the parent flow.
    pub rect: Rect,
    /// Extent of the in-flow content.
    pub content_size: Vec2,
    /// Largest valid scroll position: `(content_size - rect.size()).max(0)`.
    pub max_offset: Vec2,
    /// Whether the pointer was over the viewport when the scrollable was built.
    pub hovered: bool,
}
