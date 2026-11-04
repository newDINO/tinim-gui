//! Backend-neutral input for one frame.
//!
//! Coordinates are logical points. A backend such as `tinim-winit` converts physical
//! coordinates and event edges into this snapshot.

use tinim_paint::{Pos2, Vec2};

/// The modifier keys held at the end of the frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub logo: bool,
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
    /// Scroll delta in logical points.
    pub scroll: Vec2,
    pub modifiers: Modifiers,
    /// Text produced by keyboard input during this frame.
    pub text: String,
}
