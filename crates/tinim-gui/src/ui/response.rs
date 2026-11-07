//! Results and caller-owned state exchanged with widgets.

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
    /// Current scroll position. Zero shows the content origin.
    pub offset: Vec2,
    /// Content size measured on the previous frame, if any.
    ///
    /// When this is known, the scroll position is clamped before layout, so an out-of-range
    /// position is corrected in the same frame. It is `None` on the first frame.
    pub content_size: Option<Vec2>,
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
