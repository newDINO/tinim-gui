//! The shared result type returned by widgets.

use tinim_paint::Rect;

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
