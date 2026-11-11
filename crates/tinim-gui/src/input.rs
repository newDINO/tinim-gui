//! Backend-neutral input for one frame.
//!
//! Coordinates are logical points. A backend such as `tinim-winit` converts physical
//! coordinates and event edges into this snapshot.

use std::ops::Range;

use tinim_paint::{Pos2, Vec2};

/// The modifier keys held at the end of the frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub logo: bool,
}

impl Modifiers {
    /// Whether a Ctrl/Cmd shortcut is active without Alt.
    ///
    /// AltGr commonly arrives as Ctrl+Alt, so Alt is excluded to keep it available for text input.
    pub fn command(&self) -> bool {
        (self.ctrl || self.logo) && !self.alt
    }
}

/// A non-text key press.
///
/// Printable text is delivered separately through [`RawInput::text`]. Keys are reported here so
/// widgets such as text editors can handle editing commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Backspace,
    Delete,
    Enter,
    Escape,
    Tab,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    /// A character key, normalized to lowercase, for shortcuts such as Ctrl+A.
    Character(char),
}

/// Text produced by an input method editor (IME).
///
/// While the IME is composing, `preedit` contains the composition string. It is displayed by the
/// focused text widget but is not part of the edited string until the IME commits it. A backend
/// should keep the current preedit in every frame until the IME changes or clears it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImeInput {
    /// Composition text, shown inline in the focused text input.
    ///
    /// Empty when the IME is not composing.
    pub preedit: String,
    /// Byte range within `preedit` selected by the IME.
    ///
    /// The caret is drawn at the end of the range. `None` means the IME did not report a cursor.
    pub preedit_selection: Option<Range<usize>>,
    /// Text committed by the IME during this frame.
    ///
    /// This is inserted into the focused text input at the caret, replacing any selection.
    pub commit: String,
}

impl ImeInput {
    /// Whether the IME is currently composing text.
    pub fn is_composing(&self) -> bool {
        !self.preedit.is_empty()
    }
}

/// Everything the GUI needs to know about input for one frame.
///
/// `primary_pressed` and `primary_released` are edges: they are true only on the frame in
/// which the button changed state. `primary_down` is the state at the end of the frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RawInput {
    /// Pointer position in logical points, relative to the window's top-left corner.
    pub pointer: Option<Pos2>,
    /// Whether the primary mouse button is currently held.
    pub primary_down: bool,
    /// The primary button was pressed during this frame.
    pub primary_pressed: bool,
    /// The primary button was released during this frame.
    pub primary_released: bool,
    /// Scroll delta in logical points. Positive x scrolls right, positive y scrolls up.
    pub scroll: Vec2,
    pub modifiers: Modifiers,
    /// Text produced by keyboard input during this frame.
    pub text: String,
    /// Text composed or committed by an input method editor during this frame.
    pub ime: ImeInput,
    /// Non-text key presses during this frame, in event order.
    pub keys: Vec<Key>,
    /// Clipboard text supplied in response to a paste request.
    ///
    /// A backend reads the clipboard when it observes Ctrl/Cmd+V while the previous frame
    /// reported [`PlatformOutput::text_input_focused`](crate::PlatformOutput::text_input_focused).
    /// Native reads are immediate; browser reads are asynchronous, so the text may arrive in a
    /// later frame. `None` means no paste was requested or the clipboard was unavailable.
    pub pasted_text: Option<String>,
    /// Whether `scroll` came from dragging the pointer (a touch gesture) rather than a wheel.
    pub scroll_dragging: bool,
    /// Seconds since the previous frame. Used to animate scroll inertia; zero when unknown.
    pub dt: f32,
}

impl RawInput {
    /// Whether a Ctrl/Cmd shortcut is active without Alt.
    ///
    /// AltGr commonly arrives as Ctrl+Alt, so Alt is excluded to keep it available for text input.
    pub fn command(&self) -> bool {
        self.modifiers.command()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_accepts_ctrl_or_logo_but_not_altgr() {
        assert!(
            Modifiers {
                ctrl: true,
                ..Default::default()
            }
            .command()
        );
        assert!(
            Modifiers {
                logo: true,
                ..Default::default()
            }
            .command()
        );
        assert!(
            !Modifiers {
                ctrl: true,
                alt: true,
                ..Default::default()
            }
            .command()
        );
        assert!(!Modifiers::default().command());
    }
}
