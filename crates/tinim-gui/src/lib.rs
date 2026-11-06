//! A tiny immediate-mode GUI built on [`tinim_paint`] and [`tinim_text`].
//!
//! # Current design
//! There are no widget ids or built-in centralized widget state storage.
//!
//! 1. Widgets are made stateless when possible.
//! A button only reports a press edge, click/press-release pairing is not implemented.
//! A label can't be selected.
//! Future versions may implement stateful version of these widgets to support these features.
//!
//! 2. When a widget truly need states, they are provided by the user.
//! For example, [`Ui::scrollable`] takes `&mut ScrollState` as an argument.
//! [`ScrollState`] contains necessary states for the scrollable, e.g. scroll position.
//!
//! 3. Useful macros are provied to create states in place using TLS (thread local storage).
//! User can use [`scrollable!`] macro to create a TLS for [`ScrollState`].
//! This is extremely convenient, but with certain limitations:
//!
//! - TLS is created at the place where the macro is expanded.
//! If the expanded code is executed multiple times in one frame
//! (for example, by using the macro in a loop or the function using the macro is called twice),
//! the created widgets will share the same state.
//!
//! - TLS is thread local,
//! using it across multiple threads will create many states that are not consistent.
//!
//! - TLS can't be freed,
//! states are preserved even when the widget is no longer used.
//! And when the widget is rendered again,
//! it will get its state from the TLS rather than creating a new state.
//! This may change in the future by also storing a frame count in the TLS:
//! if `current_frame.wrapping_sub(stored_frame) > 1` the state is reset.

mod context;
mod input;
mod style;
mod text_cache;
mod ui;

pub use tinim_paint::{Color32, PaintList, Pos2, Rect, TextureId, TexturesDelta, Vec2, pos2, vec2};
pub use tinim_text;

pub use context::{Context, Frame};
pub use input::{Modifiers, RawInput};
pub use style::{ButtonStyle, Style};
pub use ui::{PaneResponse, Response, ScrollResponse, ScrollState, Ui};
