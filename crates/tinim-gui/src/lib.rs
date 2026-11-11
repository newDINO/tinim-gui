//! A tiny immediate-mode GUI built on [`tinim_paint`], [`tinim_text`] and [`tinim_paint_text`].
//!
//! # Current design
//! There are no widget ids or built-in centralized widget state storage.
//!
//! 1. Widgets are made stateless when possible.
//!    A button only reports a press edge, click/press-release pairing is not implemented.
//!    A label can't be selected.
//!    Future versions may implement stateful version of these widgets to support these features.
//!
//! 2. When a widget truly needs state, it is provided by the user.
//!    For example, [`Ui::scrollable`] takes `&mut ScrollState` as an argument.
//!    [`ScrollState`] contains necessary states for the scrollable, e.g. scroll position.
//!    [`Ui::text_edit_singleline`] takes the edited `&mut String` plus a [`TextEditState`]
//!    for the caret, selection and scroll offset.
//!
//! 3. Useful macros are provided to create states in place using TLS (thread local storage).
//!    User can use [`scrollable!`] macro to create a TLS for [`ScrollState`].
//!    This is extremely convenient, but with certain limitations:
//!
//! - TLS is created at the place where the macro is expanded.
//!   If the expanded code is executed multiple times in one frame
//!   (for example, by using the macro in a loop or the function using the macro is called twice),
//!   the created widgets will share the same state.
//!
//! - TLS is thread local,
//!   using it across multiple threads will create many states that are not consistent.
//!
//! - TLS can't be freed,
//!   states are preserved even when the widget is no longer used.
//!   And when the widget is rendered again,
//!   it will get its state from the TLS rather than creating a new state.
//!   This may change in the future by also storing a frame count in the TLS:
//!   if `current_frame.wrapping_sub(stored_frame) > 1` the state is reset.
//!
//! ## Hashing
//!
//! Internal caches use `rustc-hash` by default. Disable the default `rustc-hash` feature to use
//! the standard library's randomly seeded hasher instead.

mod context;
mod cursor;
mod hash;
mod input;
mod platform;
mod style;
mod text_cache;
mod ui;

pub use tinim_paint::{Color32, PaintList, Pos2, Rect, TextureId, TexturesDelta, Vec2, pos2, vec2};
pub use tinim_paint_text;
pub use tinim_text;

pub use context::{Context, FullOutput};
pub use cursor::CursorIcon;
pub use input::{Key, Modifiers, RawInput};
pub use platform::PlatformOutput;
pub use style::{
    ButtonStyle, CheckboxStyle, NumberInputStyle, RadioStyle, SliderStyle, Style, TextEditStyle,
};
pub use ui::{
    Align, FlowConfig, NumberInputResponse, NumberInputState, PaneResponse, Response,
    ScrollResponse, ScrollState, TextEditResponse, TextEditState, Ui,
};
