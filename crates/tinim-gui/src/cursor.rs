//! Backend-neutral mouse cursor icons.

/// The mouse cursor a widget wants while the pointer is over it.
///
/// Widgets request an icon through [`Ui::set_cursor_icon`](crate::Ui::set_cursor_icon); the
/// backend applies the icon in [`FullOutput::platform`](crate::FullOutput::platform) to the window
/// after building the frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorIcon {
    /// The platform's default arrow cursor.
    #[default]
    Default,
    /// A pointing-hand cursor, used by clickable widgets such as buttons.
    Pointer,
    /// An I-beam cursor, used by text editors.
    Text,
}
