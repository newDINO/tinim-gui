//! Non-rendering outputs for the platform or application.

use crate::cursor::CursorIcon;

/// Requests that the platform backend apply after building a frame.
///
/// Rendering data such as the paint list and texture uploads lives in
/// [`FullOutput`](crate::FullOutput). This type covers the parts that need a window or application
/// integration, such as the mouse cursor. Future fields can be added here without changing
/// [`FullOutput`](crate::FullOutput) itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlatformOutput {
    /// Mouse cursor requested by the widgets built this frame.
    ///
    /// Interactable widgets request an icon while the pointer is over them. A backend should
    /// apply this to its window only when it differs from the icon it applied previously.
    pub cursor_icon: CursorIcon,
}
